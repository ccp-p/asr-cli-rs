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
use tokio::signal;
use std::{collections::HashMap, path::PathBuf};
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
    
      // 构建配置参数字典
      let mut config_params = HashMap::new();
    
    // 直接添加已有的值，不需要使用 if let Some 模式
        config_params.insert("media_folder".to_string(), serde_json::to_value(&cli.media_folder)?);
        config_params.insert("output_folder".to_string(), serde_json::to_value(&cli.output_folder)?);
        config_params.insert("max_retries".to_string(), serde_json::to_value(cli.max_retries)?);
        config_params.insert("max_workers".to_string(), serde_json::to_value(cli.max_workers)?);
        config_params.insert("use_jianying_first".to_string(), serde_json::to_value(cli.use_jianying_first)?);
        config_params.insert("use_kuaishou".to_string(), serde_json::to_value(cli.use_kuaishou)?);
        config_params.insert("use_bcut".to_string(), serde_json::to_value(cli.use_bcut)?);
        config_params.insert("format_text".to_string(), serde_json::to_value(cli.format_text)?);
        config_params.insert("include_timestamps".to_string(), serde_json::to_value(cli.include_timestamps)?);
        config_params.insert("show_progress".to_string(), serde_json::to_value(cli.show_progress)?);
        config_params.insert("process_video".to_string(), serde_json::to_value(cli.process_video)?);
        config_params.insert("extract_audio_only".to_string(), serde_json::to_value(cli.extract_audio_only)?);
        config_params.insert("watch_mode".to_string(), serde_json::to_value(cli.watch_mode)?);
        
  
        // 创建处理器控制器
        let controller = ProcessorController::new(
            cli.config.as_deref(), // 注意: cli.config 在您的 Cli 结构体中似乎没有定义
            Some(config_params),
        )?;

       // 创建中断处理任务
       let controller_clone = controller.clone();
       let interrupt_handler = tokio::spawn(async move {
           if let Ok(()) = signal::ctrl_c().await {
               log::warn!("\n\n⚠️ 接收到中断信号，正在安全终止程序...\n稍等片刻，正在保存已处理的数据...\n");
               controller_clone.set_interrupt_flag(true);
           }
       });
          // 启动处理
    let processing = controller.start_processing();
    
      log::info!("\n程序执行完毕。");
      Ok(())
}