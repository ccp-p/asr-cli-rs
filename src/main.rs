mod cli;
mod config;
mod core;
mod asr;
mod processing;
mod ui;
mod controller;
mod logging;
mod error;

use clap::Parser;
use anyhow::Context;
use std::path::PathBuf;
use controller::ProcessorController;

#[derive(Parser, Debug)]
#[clap(author, version, about = "音频处理和转写工具")]
struct Cli {
    /// 媒体文件夹路径
    #[clap(long, default_value = "D:/download/")]
    media_folder: PathBuf,
    
    /// 输出文件夹路径
    #[clap(long, default_value = "D:/download/dest/")]
    output_folder: PathBuf,
    
    /// 最大重试次数
    #[clap(long, default_value = "3")]
    max_retries: u32,
    
    /// 最大工作线程数
    #[clap(long, default_value = "4")]
    max_workers: u32,
    
    /// 是否优先使用剪映ASR
    #[clap(long)]
    use_jianying_first: bool,
    
    /// 是否使用快手ASR
    #[clap(long)]
    use_kuaishou: bool,
    
    /// 是否使用必剪ASR
    #[clap(long)]
    use_bcut: bool,
    
    /// 是否格式化文本
    #[clap(long)]
    format_text: bool,
    
    /// 是否包含时间戳
    #[clap(long)]
    include_timestamps: bool,
    
    /// 是否显示进度条
    #[clap(long)]
    show_progress: bool,
    
    /// 是否处理视频
    #[clap(long)]
    process_video: bool,
    
    /// 是否仅提取音频
    #[clap(long)]
    extract_audio_only: bool,
    
    /// 是否启用监控模式
    #[clap(long)]
    watch_mode: bool,
    
    /// 日志文件路径
    #[clap(long)]
    log_file: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    // 解析命令行参数
    let cli = Cli::parse();
    
    // 设置日志
    logging::setup_logging(cli.log_file.as_deref())
        .context("无法设置日志系统")?;
    
  
    
    // 设置代理(如果需要)
    // Rust中设置环境变量
    std::env::set_var("HTTP_PROXY", "http://127.0.0.1:7890");
    std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:7890");
    
    // 为了让reqwest库使用这些代理，还需要确保它的配置使用系统代理
    
    // 创建处理器控制器
    match ProcessorController::new(
        cli.media_folder,
        cli.output_folder,
        cli.max_retries,
        cli.max_workers,
        cli.use_jianying_first,
        cli.use_kuaishou,
        cli.use_bcut,
        cli.format_text,
        cli.include_timestamps,
        cli.show_progress,
        cli.process_video,
        cli.extract_audio_only,
        cli.watch_mode,
    ) {
        Ok(controller) => {
            // 开始处理
            if let Err(e) = controller.start_processing() {
                log::error!("处理过程中发生错误: {}", e);
                return Err(anyhow::anyhow!("处理过程中发生错误: {}", e));
            }
        },
        Err(e) => {
            log::error!("创建处理器控制器失败: {}", e);
            return Err(anyhow::anyhow!("创建处理器控制器失败: {}", e));
        }
    }
    
    log::info!("程序执行完毕。");
    Ok(())
}