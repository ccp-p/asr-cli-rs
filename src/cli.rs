use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about = "音频处理和转写工具")]
pub struct Cli {
    /// 媒体文件夹路径
    #[clap(long, default_value = "D:/download/")]
    pub media_folder: PathBuf,
    
    /// 输出文件夹路径
    #[clap(long, default_value = "D:/download/dest/")]
    pub output_folder: PathBuf,
    
    /// 最大重试次数
    #[clap(long, default_value = "3")]
    pub max_retries: u32,
    
    /// 最大工作线程数
    #[clap(long, default_value = "4")]
    pub max_workers: u32,
    
    /// 是否优先使用剪映ASR
    #[clap(long)]
    pub use_jianying_first: bool,
    
    /// 是否使用快手ASR
    #[clap(long)]
    pub use_kuaishou: bool,
    
    /// 是否使用必剪ASR
    #[clap(long)]
    pub use_bcut: bool,
    
    /// 是否格式化文本
    #[clap(long)]
    pub format_text: bool,
    
    /// 是否包含时间戳
    #[clap(long)]
    pub include_timestamps: bool,
    
    /// 是否显示进度条
    #[clap(long)]
    pub show_progress: bool,
    
    /// 是否处理视频
    #[clap(long)]
    pub process_video: bool,
    
    /// 是否仅提取音频
    #[clap(long)]
    pub extract_audio_only: bool,
    
    /// 是否启用监控模式
    #[clap(long)]
    pub watch_mode: bool,
    
    /// 日志文件路径
    #[clap(long)]
    pub log_file: Option<PathBuf>,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}