//! `nucleo` 后台模糊搜索的轻量封装。

use std::sync::Arc;

use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Nucleo, Utf32String};

use crate::track::Track;

pub struct SearchIndex {
    matcher: Nucleo<usize>,
    query: String,
    results: Vec<usize>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self {
            matcher: Nucleo::new(Config::DEFAULT, Arc::new(|| ()), None, 1),
            query: String::new(),
            results: Vec::new(),
        }
    }

    pub fn replace_tracks(&mut self, tracks: &[Track]) {
        self.matcher.restart(true);
        let injector = self.matcher.injector();
        for (index, track) in tracks.iter().enumerate() {
            let columns = track.searchable_columns();
            let searchable = columns.join("  ");
            injector.push(index, move |_, matcher_columns| {
                matcher_columns[0] = Utf32String::from(searchable);
            });
        }
        drop(injector);
        self.reparse(false);
        self.results = (0..tracks.len()).collect();
    }

    pub fn set_query(&mut self, query: String) {
        let append = query.starts_with(&self.query);
        self.query = query;
        self.reparse(append);
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn tick(&mut self) -> bool {
        let status = self.matcher.tick(0);
        if status.changed || (!status.running && self.results.is_empty()) {
            self.results = self
                .matcher
                .snapshot()
                .matched_items(..)
                .map(|item| *item.data)
                .collect();
        }
        status.changed
    }

    pub fn results(&self) -> &[usize] {
        &self.results
    }

    fn reparse(&mut self, append: bool) {
        self.matcher.pattern.reparse(
            0,
            &self.query,
            CaseMatching::Ignore,
            Normalization::Smart,
            append,
        );
        self.matcher.tick(0);
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::*;

    fn track(title: &str, artist: &str, album: &str, path: &str) -> Track {
        Track {
            path: PathBuf::from(path),
            relative_path: PathBuf::from(path),
            title: title.to_owned(),
            artist: Some(artist.to_owned()),
            album: Some(album.to_owned()),
            duration: Some(Duration::from_secs(1)),
            format: Some("FLAC".to_owned()),
            file_size: 1,
            modified_ns: 1,
        }
    }

    #[test]
    fn searches_all_metadata_fields_case_insensitively() {
        let tracks = vec![
            track("夜曲", "周杰伦", "十一月的萧邦", "jay/01.flac"),
            track("Hello", "Adele", "25", "adele/hello.flac"),
        ];
        let mut search = SearchIndex::new();
        search.replace_tracks(&tracks);
        assert_search(&mut search, "ADELE", &[1]);
        assert_search(&mut search, "萧邦", &[0]);
        assert_search(&mut search, "jay/01", &[0]);
    }

    fn assert_search(search: &mut SearchIndex, query: &str, expected: &[usize]) {
        search.set_query(query.to_owned());
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            search.tick();
            if search.results() == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(search.results(), expected);
    }
}
