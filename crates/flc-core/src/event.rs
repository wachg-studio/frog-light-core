use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// 启动流程中的一个阶段；`ms` 为该阶段完成时的耗时。
    Stage { name: String, ms: Option<u64> },
    /// `active` 是此刻正在下载的几个文件名，给“任务详情”显示用。
    Progress { label: String, done: u64, total: u64, files_done: u32, files_total: u32, active: Vec<String> },
    Log { line: String },
    Started { pid: u32 },
    /// 游戏已加载到资源就绪之后（窗口基本已经出现）。
    Ready,
    /// `ready` 表示退出前是否到达过 Ready；`tail` 是最后几十行输出，用于解释异常退出。
    Exited { code: Option<i32>, ready: bool, secs: u64, tail: Vec<String> },
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn stage(sink: &Sink, name: &str) {
    sink(Event::Stage { name: name.into(), ms: None });
}
