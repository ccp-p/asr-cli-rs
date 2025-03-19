mod cli;
mod config;
mod core;
mod asr;
mod processing;
mod ui;
mod controller;

use anyhow::{Context, Result};
use std::env;
use tracing::{info, warn, error, Level};
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};
use controller::Controller;
use core::file_utils::check_ffmpeg_available;
use indicatif::{ProgressBar, ProgressStyle};

fn check_dependencies() -> Result<bool> {
    info!("检查系统依赖...");
    
    // 检查 FFmpeg
    if !check_ffmpeg_available() {
        warn!("警告: 未检测到FFmpeg，转换视频需要FFmpeg支持");
        println!("\n警告: 未检测到FFmpeg，转换视频需要FFmpeg支持");
        println!("请安装FFmpeg: https://ffmpeg.org/download.html");
        println!("安装后确保将FFmpeg添加到系统PATH中\n");
        return Ok(false);
    }
    
    info!("所有依赖检查通过");
    Ok(true)
}

fn setup_logging() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_span_events(FmtSpan::CLOSE)
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .pretty();
    
    subscriber.init();
    Ok(())
}

fn set_proxy() {
    info!("设置系统代理...");
    env::set_var("HTTP_PROXY", "http://127.0.0.1:7890");
    env::set_var("HTTPS_PROXY", "http://127.0.0.1:7890");
}

#[tokio::main]
async fn main() -> Result<()> {
    // 设置日志系统
    setup_logging()?;
    info!("程序启动...");
    
    // 解析命令行参数
    let cli_args = cli::parse_args();
    
    // 检查依赖
    if !check_dependencies()? {
        error!("依赖检查失败，程序退出");
        std::process::exit(1);
    }
    
    // 设置代理
    set_proxy();
    
    info!("初始化音频处理器...");
    
    // 创建并配置控制器
    let mut controller = Controller::new(
        "D:/download/",                // media_folder
        "D:/download/dest/",           // output_folder
        3,                             // max_retries
        4,                             // max_workers
        true,                          // use_jianying_first
        true,                          // use_kuaishou
        true,                          // use_bcut
        true,                          // format_text
        true,                          // include_timestamps
        true,                          // show_progress
        true,                          // process_video
        false,                         // extract_audio_only
        true,                          // watch_mode
    ).context("创建控制器失败")?;
    
    // 开始处理
    info!("开始处理任务...");
    match controller.start_processing().await {
        Ok(_) => info!("所有任务处理完成"),
        Err(e) => {
            error!("处理过程中出错: {}", e);
            error!("详细错误信息: {:?}", e);
        }
    }
    
    info!("程序执行完毕");
    
    Ok(())
}