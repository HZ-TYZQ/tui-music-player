//! 临时播放队列弹层的交互和队列编辑行为。

use crossterm::event::KeyCode;

use super::{App, BagUpdate, Overlay};

impl App {
    pub(super) fn handle_queue_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('Q') => self.overlay = Overlay::None,
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.queue.is_empty() {
                    self.queue_selected = (self.queue_selected + 1).min(self.queue.len() - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.queue_selected = self.queue_selected.saturating_sub(1);
            }
            KeyCode::Char('J') => self.move_queue_selected(1),
            KeyCode::Char('K') => self.move_queue_selected(-1),
            KeyCode::Enter => self.play_queue_selected(),
            KeyCode::Char('d') => self.remove_queue_selected(),
            KeyCode::Char('c') => self.clear_queue(),
            _ => {}
        }
    }

    /// 跳到队列中的某一项：它之前的条目视为已跳过，直接从队列丢弃。
    fn play_queue_selected(&mut self) {
        if self.queue_selected >= self.queue.len() {
            self.message = Some("队列是空的".to_owned());
            return;
        }
        self.queue.drain(..self.queue_selected);
        self.queue_selected = 0;
        let Some(path) = self.queue.pop_front() else {
            return;
        };
        if self.play_path(&path, true, BagUpdate::Reanchor) {
            self.overlay = Overlay::None;
            return;
        }
        // 该条目无法播放时沿队列继续向后尝试，与自动切歌的跳过行为一致。
        self.play_next(false);
        if self.playing_index.is_some() {
            self.overlay = Overlay::None;
        }
    }

    fn remove_queue_selected(&mut self) {
        if self.queue.remove(self.queue_selected).is_none() {
            self.message = Some("队列是空的".to_owned());
            return;
        }
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        self.message = Some(format!("已从队列移除，队列中还有 {} 首", self.queue.len()));
    }

    fn clear_queue(&mut self) {
        if self.queue.is_empty() {
            self.message = Some("队列是空的".to_owned());
            return;
        }
        let removed = self.queue.len();
        self.queue.clear();
        self.queue_selected = 0;
        self.message = Some(format!("已清空队列（{removed} 首），音乐文件未受影响"));
    }

    /// 上移或下移选中条目，选择跟随该条目移动。
    fn move_queue_selected(&mut self, offset: isize) {
        let target = self.queue_selected as isize + offset;
        if target < 0 || target as usize >= self.queue.len() {
            return;
        }
        let target = target as usize;
        self.queue.swap(self.queue_selected, target);
        self.queue_selected = target;
    }
}
