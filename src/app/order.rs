//! 曲库内的播放顺序：顺序、列表循环、单曲循环和随机袋。
//!
//! 这里只和曲库下标打交道，不碰播放器、队列和提示，因此能脱离音频单独测试。
//! 曲库长度、当前曲目和播放模式都由调用方传入；随机数源在构造时注入。

use crate::track::{PlaybackMode, RepeatMode};

/// 随机袋：一轮里每首歌恰好出现一次，一轮放完再洗下一轮。
#[derive(Debug)]
pub(crate) struct PlaybackOrder {
    bag: Vec<usize>,
    /// 下一首在袋中的位置。
    cursor: usize,
    rng_state: u64,
}

impl PlaybackOrder {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            bag: Vec::new(),
            cursor: 0,
            // xorshift 的状态为 0 时会一直停在 0。
            rng_state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 曲库中 `current` 之后该播哪一首。`natural_end` 表示上一首是自然播完的，
    /// 只有这时单曲循环才重播同一首；播放出错时单曲循环也照常往后走。
    pub(crate) fn next(
        &mut self,
        len: usize,
        current: Option<usize>,
        natural_end: bool,
        mode: PlaybackMode,
    ) -> Option<usize> {
        if mode.shuffle {
            if natural_end && mode.repeat == RepeatMode::One {
                return current;
            }
            return self.next_from_bag(len, current, mode.repeat);
        }
        match mode.repeat {
            RepeatMode::One if natural_end => current,
            RepeatMode::All | RepeatMode::One if len > 0 => Some((current.unwrap_or(0) + 1) % len),
            RepeatMode::All | RepeatMode::One => None,
            RepeatMode::None => match current {
                Some(index) if index + 1 < len => Some(index + 1),
                None if len > 0 => Some(0),
                _ => None,
            },
        }
    }

    /// 不动随机袋地回答还有没有下一首，供系统媒体控制显示。
    pub(crate) fn has_next(&self, len: usize, current: Option<usize>, mode: PlaybackMode) -> bool {
        if len == 0 {
            return false;
        }
        if mode.repeat != RepeatMode::None {
            return true;
        }
        if mode.shuffle {
            // 袋子还没建好时，第一次取下一首会以当前曲目为首重新洗一轮。
            return self.bag.is_empty() || self.cursor < self.bag.len();
        }
        match current {
            Some(index) => index + 1 < len,
            None => true,
        }
    }

    /// 以 `current` 为本轮第一首重新洗牌，其余曲目随机排在后面。
    pub(crate) fn reanchor(&mut self, len: usize, current: usize) {
        if len == 0 {
            self.clear();
            return;
        }
        let current = current.min(len - 1);
        let mut rest: Vec<usize> = (0..len).filter(|index| *index != current).collect();
        self.shuffle(&mut rest);
        self.bag = std::iter::once(current).chain(rest).collect();
        self.cursor = 1;
    }

    pub(crate) fn clear(&mut self) {
        self.bag.clear();
        self.cursor = 0;
    }

    #[cfg(test)]
    pub(crate) fn bag(&self) -> &[usize] {
        &self.bag
    }

    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    fn next_from_bag(
        &mut self,
        len: usize,
        current: Option<usize>,
        repeat: RepeatMode,
    ) -> Option<usize> {
        if len == 0 {
            return None;
        }
        if self.bag.is_empty() {
            self.reanchor(len, current.unwrap_or(0));
        }
        if self.cursor >= self.bag.len() {
            match repeat {
                RepeatMode::None => return None,
                RepeatMode::All | RepeatMode::One => {
                    let avoid = self.bag.last().copied().or(current).unwrap_or(0);
                    self.reshuffle_round(len, avoid);
                }
            }
        }
        let next = self.bag.get(self.cursor).copied()?;
        self.cursor += 1;
        Some(next)
    }

    /// 新一轮整体洗牌；第一首避开上一轮的最后一首，接缝处不会连播同一首。
    fn reshuffle_round(&mut self, len: usize, avoid_first: usize) {
        let mut order: Vec<usize> = (0..len).collect();
        self.shuffle(&mut order);
        if order.len() > 1 && order[0] == avoid_first {
            let swap = (1..order.len())
                .find(|index| order[*index] != avoid_first)
                .unwrap_or(1);
            order.swap(0, swap);
        }
        self.bag = order;
        self.cursor = 0;
    }

    fn shuffle(&mut self, items: &mut [usize]) {
        if items.len() < 2 {
            return;
        }
        for index in (1..items.len()).rev() {
            let other = self.random_index(index + 1);
            items.swap(index, other);
        }
    }

    fn random_index(&mut self, upper: usize) -> usize {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as usize) % upper
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(repeat: RepeatMode, shuffle: bool) -> PlaybackMode {
        PlaybackMode { repeat, shuffle }
    }

    #[test]
    fn sequential_modes_walk_the_library_in_order() {
        let mut order = PlaybackOrder::new(1);
        let none = mode(RepeatMode::None, false);
        assert_eq!(order.next(3, None, false, none), Some(0));
        assert_eq!(order.next(3, Some(1), false, none), Some(2));
        assert_eq!(order.next(3, Some(2), false, none), None);
        assert!(!order.has_next(3, Some(2), none));

        let all = mode(RepeatMode::All, false);
        assert_eq!(order.next(3, Some(2), false, all), Some(0));

        // 单曲循环只在自然播完时重播；出错时照常往后走。
        let one = mode(RepeatMode::One, false);
        assert_eq!(order.next(3, Some(1), true, one), Some(1));
        assert_eq!(order.next(3, Some(1), false, one), Some(2));
        assert_eq!(order.next(0, None, false, all), None);
    }

    #[test]
    fn a_shuffle_round_plays_every_track_once_starting_after_the_anchor() {
        let mut order = PlaybackOrder::new(7);
        let shuffle = mode(RepeatMode::None, true);
        order.reanchor(6, 3);
        assert_eq!(order.bag()[0], 3);
        let mut played: Vec<usize> = (0..5)
            .map(|_| order.next(6, Some(3), false, shuffle).unwrap())
            .collect();
        assert!(!played.contains(&3));
        played.sort_unstable();
        assert_eq!(played, [0, 1, 2, 4, 5]);
        assert_eq!(order.next(6, Some(3), false, shuffle), None);
        assert!(!order.has_next(6, Some(3), shuffle));
    }

    #[test]
    fn repeating_shuffle_never_plays_the_same_track_across_the_seam() {
        for seed in 1..200 {
            let mut order = PlaybackOrder::new(seed);
            let shuffle = mode(RepeatMode::All, true);
            order.reanchor(3, 0);
            let mut last = 0;
            for _ in 0..12 {
                let next = order.next(3, Some(last), false, shuffle).unwrap();
                assert_ne!(next, last, "seed {seed}");
                last = next;
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_order() {
        let shuffle = mode(RepeatMode::All, true);
        let run = || {
            let mut order = PlaybackOrder::new(42);
            (0..10)
                .map(|_| order.next(8, Some(0), false, shuffle))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
