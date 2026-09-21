//! LRC 同步歌词：解析、按播放位置定位当前行，以及从同名 `.lrc` 或内嵌标签读取。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::ItemKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time: Duration,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lyrics {
    lines: Vec<LyricLine>,
}

impl Lyrics {
    /// 只收带时间标签的行；一行都没有就不是同步歌词，返回 None。
    ///
    /// 支持一行多个时间标签、`[offset:±ms]`，并去掉逐字歌词的 `<mm:ss.xx>` 标签。
    /// `ti`、`ar` 等元数据标签被忽略。
    pub fn parse(text: &str) -> Option<Self> {
        let mut offset_ms: i64 = 0;
        let mut lines = Vec::new();
        for raw in text.lines() {
            let mut rest = raw.trim();
            let mut times = Vec::new();
            while let Some(after) = rest.strip_prefix('[') {
                let Some(end) = after.find(']') else {
                    break;
                };
                let tag = &after[..end];
                if let Some(time) = parse_timestamp(tag) {
                    times.push(time);
                } else if let Some(value) = tag.strip_prefix("offset:") {
                    offset_ms = value.trim().parse().unwrap_or(offset_ms);
                }
                rest = after[end + 1..].trim_start();
            }
            if times.is_empty() {
                continue;
            }
            let text = strip_word_timestamps(rest);
            lines.extend(times.into_iter().map(|time| LyricLine {
                time,
                text: text.clone(),
            }));
        }
        if lines.is_empty() {
            return None;
        }
        // 正 offset 让歌词提前出现。
        for line in &mut lines {
            let shifted = i64::try_from(line.time.as_millis()).unwrap_or(i64::MAX) - offset_ms;
            line.time = Duration::from_millis(shifted.max(0) as u64);
        }
        // 稳定排序：同一时刻的多行（如原文与译文）保持文件中的先后。
        lines.sort_by_key(|line| line.time);
        Some(Self { lines })
    }

    pub fn lines(&self) -> &[LyricLine] {
        &self.lines
    }

    /// 已经开始的最后一行；第一行之前返回 None。
    pub fn current_index(&self, position: Duration) -> Option<usize> {
        self.lines
            .partition_point(|line| line.time <= position)
            .checked_sub(1)
    }

    /// 先找音频旁的同名 `.lrc`，再找内嵌标签里的 LRC 文本。
    /// 没有歌词不是错误；`.lrc` 存在却读不了才返回 Err，让界面说明原因。
    pub fn load_for(track: &Path) -> Result<Option<Self>, String> {
        if let Some(path) = sidecar_path(track) {
            let bytes = fs::read(&path)
                .map_err(|error| format!("无法读取歌词 {}: {error}", file_label(&path)))?;
            let text = decode(&bytes).ok_or_else(|| {
                format!(
                    "歌词 {} 不是 UTF-8 或 UTF-16 编码，已忽略",
                    file_label(&path)
                )
            })?;
            return Ok(Self::parse(&text));
        }
        Ok(embedded(track))
    }
}

/// `mm:ss`、`mm:ss.xx`、`mm:ss.xxx` 以及少见的 `mm:ss:xx`。
fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (minutes, rest) = tag.split_once(':')?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (rest, None),
    };
    let all_digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(minutes) || !all_digits(seconds) {
        return None;
    }
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: u64 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let millis = match fraction {
        None => 0,
        Some(fraction) if all_digits(fraction) => {
            // 只取前三位：百分之一秒和毫秒都常见，个别文件写到微秒。
            let digits = &fraction[..fraction.len().min(3)];
            let value: u64 = digits.parse().ok()?;
            value * 10_u64.pow(3 - digits.len() as u32)
        }
        Some(_) => return None,
    };
    Some(Duration::from_millis(
        (minutes * 60 + seconds) * 1000 + millis,
    ))
}

fn strip_word_timestamps(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) if parse_timestamp(&rest[start + 1..start + end]).is_some() => {
                rest = &rest[start + end + 1..];
            }
            _ => {
                result.push('<');
                rest = &rest[start + 1..];
            }
        }
    }
    result.push_str(rest);
    result.trim().to_owned()
}

fn sidecar_path(track: &Path) -> Option<PathBuf> {
    ["lrc", "LRC"]
        .into_iter()
        .map(|extension| track.with_extension(extension))
        .find(|path| path.is_file())
}

fn decode(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8(rest.to_vec()).ok();
    }
    let utf16 = |rest: &[u8], from: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| from([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).ok()
    };
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        return utf16(rest, u16::from_be_bytes);
    }
    String::from_utf8(bytes.to_vec()).ok()
}

/// 内嵌歌词常以 LRC 文本放在 LYRICS / USLT / ©lyr 里；纯文本歌词没有时间，不予显示。
fn embedded(track: &Path) -> Option<Lyrics> {
    let tagged = Probe::open(track)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    tagged.tags().iter().find_map(|tag| {
        [ItemKey::Lyrics, ItemKey::UnsyncLyrics]
            .into_iter()
            .filter_map(|key| tag.get_string(key))
            .find_map(Lyrics::parse)
    })
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn parses_timestamps_metadata_and_multiple_tags() {
        let lyrics = Lyrics::parse(
            "[ti:测试]\n[ar:歌手]\n\n[00:12.50]第一行\n[01:02.345][00:05]重复\n[00:20:30]冒号小数\n纯文本行\n",
        )
        .unwrap();
        let lines: Vec<_> = lyrics
            .lines()
            .iter()
            .map(|line| (line.time, line.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            vec![
                (ms(5_000), "重复"),
                (ms(12_500), "第一行"),
                (ms(20_300), "冒号小数"),
                (ms(62_345), "重复"),
            ]
        );
    }

    #[test]
    fn current_line_follows_the_position() {
        let lyrics = Lyrics::parse("[00:01.00]a\n[00:03.00]b\n[00:03.00]b 译文\n").unwrap();
        assert_eq!(lyrics.current_index(ms(500)), None);
        assert_eq!(lyrics.current_index(ms(1_000)), Some(0));
        assert_eq!(lyrics.current_index(ms(2_999)), Some(0));
        assert_eq!(lyrics.current_index(ms(3_000)), Some(2));
        assert_eq!(lyrics.lines()[1].text, "b");
        assert_eq!(lyrics.current_index(ms(99_000)), Some(2));
    }

    #[test]
    fn offset_shifts_lines_earlier_and_never_below_zero() {
        let lyrics = Lyrics::parse("[offset:+500]\n[00:00.20]a\n[00:02.00]b\n").unwrap();
        assert_eq!(lyrics.lines()[0].time, Duration::ZERO);
        assert_eq!(lyrics.lines()[1].time, ms(1_500));

        let lyrics = Lyrics::parse("[offset:-250]\n[00:01.00]a\n").unwrap();
        assert_eq!(lyrics.lines()[0].time, ms(1_250));
    }

    #[test]
    fn word_timestamps_are_stripped_but_other_angle_brackets_stay() {
        let lyrics = Lyrics::parse("[00:01.00]<00:01.00>你 <00:01.50>好 <a>\n").unwrap();
        assert_eq!(lyrics.lines()[0].text, "你 好 <a>");
    }

    #[test]
    fn plain_text_is_not_synced_lyrics() {
        assert_eq!(Lyrics::parse("只是歌词\n没有时间"), None);
        assert_eq!(Lyrics::parse("[ar:x]\n[99:99]x"), None);
    }

    #[test]
    fn sidecar_lrc_is_found_and_decoded() {
        let temp = tempfile::tempdir().unwrap();
        let track = temp.path().join("歌.flac");
        assert_eq!(Lyrics::load_for(&track), Ok(None));

        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice("[00:01.00]带 BOM\n".as_bytes());
        fs::write(temp.path().join("歌.lrc"), &bom).unwrap();
        let lyrics = Lyrics::load_for(&track).unwrap().unwrap();
        assert_eq!(lyrics.lines()[0].text, "带 BOM");

        let mut utf16 = b"\xFF\xFE".to_vec();
        for unit in "[00:02.00]宽字符\n".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(temp.path().join("歌.lrc"), &utf16).unwrap();
        let lyrics = Lyrics::load_for(&track).unwrap().unwrap();
        assert_eq!(lyrics.lines()[0].text, "宽字符");

        // GBK 的“你好”：不是合法 UTF-8。
        fs::write(temp.path().join("歌.lrc"), b"[00:01.00]\xC4\xE3\xBA\xC3\n").unwrap();
        assert!(Lyrics::load_for(&track).unwrap_err().contains("不是 UTF-8"));
    }
}
