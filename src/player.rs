//! ffplay 子进程播放后端。
//!
//! 通过 `ffplay -nodisp -autoexit` 播放音频，
//! 用 SIGSTOP / SIGCONT 实现暂停 / 恢复。

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

pub struct Player {
    child: Option<Child>,
    state: PlayState,
    started_at: Instant,
    /// 累计暂停时长（用于计算已播放时间）
    accumulated_pause: Duration,
    pause_started: Option<Instant>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            child: None,
            state: PlayState::Stopped,
            started_at: Instant::now(),
            accumulated_pause: Duration::ZERO,
            pause_started: None,
        }
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    /// 播放指定文件，会先停掉当前播放。
    pub fn play(&mut self, path: &Path) -> io::Result<()> {
        self.stop();
        let child = Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "error"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        self.state = PlayState::Playing;
        self.started_at = Instant::now();
        self.accumulated_pause = Duration::ZERO;
        self.pause_started = None;
        Ok(())
    }

    /// 暂停 / 恢复切换。
    pub fn toggle_pause(&mut self) {
        let Some(child) = &self.child else { return };
        let pid = child.id() as i32;
        match self.state {
            PlayState::Playing => {
                // SAFETY: 向自己拥有的子进程发送信号
                if unsafe { libc::kill(pid, libc::SIGSTOP) } == 0 {
                    self.state = PlayState::Paused;
                    self.pause_started = Some(Instant::now());
                }
            }
            PlayState::Paused => {
                // SAFETY: 向自己拥有的子进程发送信号
                if unsafe { libc::kill(pid, libc::SIGCONT) } == 0 {
                    self.state = PlayState::Playing;
                    if let Some(t) = self.pause_started.take() {
                        self.accumulated_pause += t.elapsed();
                    }
                }
            }
            PlayState::Stopped => {}
        }
    }

    /// 停止播放并回收子进程。
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // 若进程处于 SIGSTOP 状态，先恢复才能被 kill 立即处理
            // SAFETY: 向自己拥有的子进程发送信号
            unsafe {
                libc::kill(child.id() as i32, libc::SIGCONT);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        self.state = PlayState::Stopped;
        self.pause_started = None;
    }

    /// 检测当前曲目是否自然播放完毕（用于自动连播）。
    pub fn poll_finished(&mut self) -> bool {
        let Some(child) = &mut self.child else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                self.child = None;
                self.state = PlayState::Stopped;
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.child = None;
                self.state = PlayState::Stopped;
                true
            }
        }
    }

    /// 已播放时长（不含暂停时间）。
    pub fn elapsed(&self) -> Duration {
        match self.state {
            PlayState::Stopped => Duration::ZERO,
            PlayState::Playing => self.started_at.elapsed() - self.accumulated_pause,
            PlayState::Paused => {
                let paused_now = self.pause_started.map(|t| t.elapsed()).unwrap_or_default();
                self.started_at
                    .elapsed()
                    .saturating_sub(self.accumulated_pause + paused_now)
            }
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
