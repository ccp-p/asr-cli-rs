mod cli;
mod config;
mod core;
mod asr;
mod processing;
mod ui;
mod controller;
mod logging;
mod error;

use anyhow::Context;
use tokio::signal;
use std::{collections::HashMap, path::PathBuf};
use controller::ProcessorController;
use crate::cli::parse_args;

fn main() -> anyhow::Result<()> {
    // 解析命令行参数
    let cli =parse_args();
    
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
        
  
        let controller = ProcessorController::new(
            None,  // 没有配置文件
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