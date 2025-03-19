use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use log::{info, warn, error};
use std::fs;
use std::thread;
use anyhow::Result;
use tokio::signal;
use walkdir::WalkDir;

use crate::core::audio_extractor::AudioExtractor;
use crate::core::file_utils::format_time_duration;
use crate::core::error::ErrorHandler;
use crate::core::config_manager::ConfigManager;
use crate::processing::transcription_processor::TranscriptionProcessor;
use crate::processing::file_processor::FileProcessor;
use crate::processing::progress_manager::ProgressManager;
use crate::asr::manager::AsrManager;

/// 处理器控制器，协调各个组件工作
pub struct ProcessorController {
    // 配置管理
    config_manager: ConfigManager,
    
    // 临时目录路径
    temp_dir: PathBuf,
    temp_segments_dir: PathBuf,
    
    // 组件
    error_handler: Arc<ErrorHandler>,
    progress_manager: Arc<ProgressManager>,
    asr_manager: Arc<AsrManager>,
    audio_extractor: Arc<AudioExtractor>,
    transcription_processor: Arc<TranscriptionProcessor>,
    file_processor: Arc<FileProcessor>,
    
    // 统计信息
    stats: Mutex<ProcessingStats>,
    
    // 中断标志
    interrupt_flag: Arc<Mutex<bool>>,
}

/// 处理统计信息
struct ProcessingStats {
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    total_files: usize,
    processed_files: usize,
    successful_files: usize,
    failed_files: usize,
    total_segments: usize,
    successful_segments: usize,
    failed_segments: usize,
}

impl ProcessorController {
    /// 创建新的处理器控制器
    pub fn new(config_file: Option<&Path>, config_params: Option<HashMap<String, serde_json::Value>>) -> Result<Self> {
        // 初始化配置管理器
        let mut config_manager = ConfigManager::new(config_file)?;
        
        // 如果提供了配置参数，更新配置
        if let Some(params) = config_params {
            config_manager.update(&params)?;
        }
        
        // 获取配置字典
        let config = config_manager.as_dict();
        
        // 创建临时目录
        let temp_dir = match config.get("temp_dir") {
            Some(value) => PathBuf::from(value.as_str().unwrap_or("")),
            None => {
                let dir = tempfile::tempdir()?.into_path();
                dir
            }
        };
        
        let temp_segments_dir = temp_dir.join("segments");
        fs::create_dir_all(&temp_segments_dir)?;
        
        // 中断标志
        let interrupt_flag = Arc::new(Mutex::new(false));
        
        // 创建错误处理器
        let error_handler = Arc::new(ErrorHandler::new(
            config.get("max_retries").and_then(|v| v.as_u64()).unwrap_or(3) as u32,
            config.get("retry_delay").and_then(|v| v.as_f64()).unwrap_or(1.0),
        ));
        
        // 创建进度管理器
        let progress_manager = Arc::new(ProgressManager::new(
            config.get("show_progress").and_then(|v| v.as_bool()).unwrap_or(true),
        ));
        
        // 创建ASR管理器
        let asr_manager = Arc::new(AsrManager::new(
            config.get("use_jianying_first").and_then(|v| v.as_bool()).unwrap_or(false),
            config.get("use_kuaishou").and_then(|v| v.as_bool()).unwrap_or(false),
            config.get("use_bcut").and_then(|v| v.as_bool()).unwrap_or(false),
        ));
        
        // 创建回调闭包
        let progress_manager_clone = Arc::clone(&progress_manager);
        let config_clone = config.clone();
        let progress_callback = move |current: usize, total: usize, message: Option<String>, context: Option<String>| {
            if !config_clone.get("show_progress").and_then(|v| v.as_bool()).unwrap_or(true) {
                return;
            }
            
            let message = message.unwrap_or_else(|| format!("处理进度: {}/{}", current, total));
            let progress_name = if let Some(ctx) = &context {
                format!("{}_progress", ctx)
            } else {
                "main_progress".to_string()
            };
            
            let progress_manager = &progress_manager_clone;
            
            if !progress_manager.has_progress_bar(&progress_name) {
                let prefix = context.clone().unwrap_or_else(|| "处理".to_string());
                progress_manager.create_progress_bar(
                    &progress_name,
                    total,
                    &prefix,
                    None,
                );
            }
            
            if let Some(bar) = progress_manager.get_progress_bar(&progress_name) {
                if bar.length() != total {
                    bar.reset(total);
                }
            }
            
            progress_manager.update_progress(
                &progress_name,
                current,
                Some(&message),
            );
            
            if current >= total {
                progress_manager.finish_progress(&progress_name, Some(&message));
            }
        };
        
        // 创建音频提取器
        let audio_extractor = Arc::new(AudioExtractor::new(
            &temp_segments_dir,
            Some(Arc::new(progress_callback.clone())),
        ));
        
        // 创建转写处理器
        let transcription_processor = Arc::new(TranscriptionProcessor::new(
            Arc::clone(&asr_manager),
            &temp_segments_dir,
            config.get("max_workers").and_then(|v| v.as_u64()).unwrap_or(4) as usize,
            config.get("max_retries").and_then(|v| v.as_u64()).unwrap_or(3) as u32,
            Some(Arc::new(progress_callback.clone())),
            Arc::clone(&interrupt_flag),
        ));
        
        // 创建文件处理器
        let file_processor = Arc::new(FileProcessor::new(
            config.get("media_folder").and_then(|v| v.as_str()).unwrap_or("").into(),
            config.get("output_folder").and_then(|v| v.as_str()).unwrap_or("").into(),
            &temp_segments_dir,
            Arc::clone(&transcription_processor),
            Arc::clone(&audio_extractor),
            Some(Arc::new(progress_callback)),
            config.get("process_video").and_then(|v| v.as_bool()).unwrap_or(true),
            config.get("extract_audio_only").and_then(|v| v.as_bool()).unwrap_or(false),
            config.get("format_text").and_then(|v| v.as_bool()).unwrap_or(true),
            config.get("include_timestamps").and_then(|v| v.as_bool()).unwrap_or(true),
            config.get("max_part_time").and_then(|v| v.as_u64()).unwrap_or(30) as u32,
            config.get("max_retries").and_then(|v| v.as_u64()).unwrap_or(3) as u32,
        ));
        
        let controller = Self {
            config_manager,
            temp_dir,
            temp_segments_dir,
            error_handler,
            progress_manager,
            asr_manager,
            audio_extractor,
            transcription_processor,
            file_processor,
            stats: Mutex::new(ProcessingStats::new()),
            interrupt_flag,
        };
        
        // 打印初始配置
        controller.config_manager.print_config();
        
        Ok(controller)
    }

    /// 更新统计信息
    fn update_stats(&self, file_stats: HashMap<&str, serde_json::Value>) {
        let mut stats = self.stats.lock().unwrap();
        
        stats.processed_files += 1;
        
        if file_stats.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            stats.successful_files += 1;
        } else {
            stats.failed_files += 1;
        }
        
        let total_segments = file_stats.get("total_segments")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
            
        let successful_segments = file_stats.get("successful_segments")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
            
        stats.total_segments += total_segments;
        stats.successful_segments += successful_segments;
        stats.failed_segments += total_segments.saturating_sub(successful_segments);
    }
    
    /// 打印最终统计信息
    fn print_final_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.end_time = Some(Instant::now());
        
        if let (Some(start), Some(end)) = (stats.start_time, stats.end_time) {
            let duration = end.duration_since(start);
            
            info!("\n处理统计:");
            info!("总计处理文件: {}/{}", stats.processed_files, stats.total_files);
            info!("成功处理: {} 个文件", stats.successful_files);
            info!("处理失败: {} 个文件", stats.failed_files);
            info!("片段统计:");
            info!("  - 总计片段: {}", stats.total_segments);
            info!("  - 成功识别: {}", stats.successful_segments);
            info!("  - 识别失败: {}", stats.failed_segments);
            
            if stats.total_segments > 0 {
                let success_rate = (stats.successful_segments as f64 / stats.total_segments as f64) * 100.0;
                info!("识别成功率: {:.1}%", success_rate);
            }
            
            info!("\n总耗时: {}", format_time_duration(&duration));
            
            // 显示ASR服务统计
            let asr_stats = self.asr_manager.get_service_stats();
            info!("\nASR服务使用统计:");
            for (name, stat) in asr_stats {
                let available_status = if stat.get("available").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "可用"
                } else {
                    "禁用"
                };
                
                info!("  {}: 使用次数 {}, 成功率 {}, 可用状态: {}", 
                    name,
                    stat.get("count").and_then(|v| v.as_u64()).unwrap_or(0),
                    stat.get("success_rate").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    available_status
                );
            }
            
            // 显示错误统计
            self.error_handler.print_error_stats();
        }
    }
    
    // 获取配置属性
    pub fn config(&self) -> HashMap<String, serde_json::Value> {
        self.config_manager.as_dict()
    }
    
    // 更新配置
    pub fn update_config(&mut self, config_dict: HashMap<String, serde_json::Value>) -> Result<()> {
        match self.config_manager.update(&config_dict) {
            Ok(_) => {
                info!("配置已更新");
                self.config_manager.print_config();
                Ok(())
            },
            Err(e) => {
                error!("更新配置失败: {}", e);
                Err(e.into())
            }
        }
    }
    
    // 保存配置
    pub fn save_config(&self, config_file: &Path) -> Result<()> {
        match self.config_manager.save_config(config_file) {
            Ok(_) => {
                info!("配置已保存到: {}", config_file.display());
                Ok(())
            },
            Err(e) => {
                error!("保存配置失败: {}", e);
                Err(e.into())
            }
        }
    }

     /// 处理文件夹中已存在的文件
     fn process_existing_files(&self) -> Result<()> {
        let config = self.config();
        let media_folder = PathBuf::from(config.get("media_folder")
            .and_then(|v| v.as_str())
            .unwrap_or(""));
            
        let mut media_files = Vec::new();
        
        // 处理MP3文件
        for entry in fs::read_dir(&media_folder)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase() == "mp3")
                .unwrap_or(false) {
                media_files.push(path);
            }
        }
        
        // 如果开启视频处理，获取视频文件
        if config.get("process_video").and_then(|v| v.as_bool()).unwrap_or(true) {
            for entry in fs::read_dir(&media_folder)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if ext_lower == "mp4" || ext_lower == "mov" || ext_lower == "avi" {
                            media_files.push(path);
                        }
                    }
                }
            }
        }
        
        if media_files.is_empty() {
            info!("没有找到需要处理的媒体文件");
            return Ok(());
        }
        
        // 更新统计信息中的总文件数
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_files = media_files.len();
        }
        
        // 创建总体进度条
        self.progress_manager.create_progress_bar(
            "total_progress",
            media_files.len(),
            "处理媒体文件",
            Some(&format!("总计 {} 个文件", media_files.len())),
        );
        
        // 处理所有文件
        for (i, filepath) in media_files.iter().enumerate() {
            let filename = filepath.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("未知文件");
                
            let success = self.error_handler.safe_execute(
                || self.file_processor.process_file(filepath),
                &format!("处理文件失败: {}", filename),
            )?;
            
            // 更新统计信息
            let mut file_stats = HashMap::new();
            file_stats.insert("success", serde_json::Value::Bool(success));
            self.update_stats(file_stats);
            
            // 更新总体进度
            self.progress_manager.update_progress(
                "total_progress",
                i + 1,
                Some(&format!("已处理 {}/{} 个文件", i+1, media_files.len())),
            );
        }
        
        // 完成总体进度
        self.progress_manager.finish_progress(
            "total_progress",
            Some(&format!("完成处理 {} 个文件", media_files.len())),
        );
        
        Ok(())
    }
    
    /// 启动监听模式
    async fn start_watch_mode(&self) -> Result<()> {
        let config = self.config();
        let media_folder = config.get("media_folder")
            .and_then(|v| v.as_str())
            .unwrap_or("");
            
        info!("启动监听模式，监控目录: {}", media_folder);
        
        let observer = self.file_processor.start_file_monitoring()?;
        
        // 等待中断信号
        signal::ctrl_c().await?;
        
        // 停止观察者
        observer.stop()?;
        info!("\n监听模式已停止");
        
        Ok(())
    }
    
    /// 启动处理流程
    pub async fn start_processing(&self) -> Result<()> {
        // 设置开始时间
        {
            let mut stats = self.stats.lock().unwrap();
            stats.start_time = Some(Instant::now());
        }
        
        let config = self.config();
        let watch_mode = config.get("watch_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
            
        // 处理流程
        if watch_mode {
            // 先处理已有文件
            self.process_existing_files()?;
            // 然后启动监控模式
            self.start_watch_mode().await?;
        } else {
            // 仅处理已有文件
            self.process_existing_files()?;
        }
        
        // 清理
        self.cleanup();
        
        // 打印最终统计
        self.print_final_stats();
        
        Ok(())
    }
    
    /// 清理资源
    fn cleanup(&self) {
        info!("清理临时文件和资源...");
        
        // 关闭所有进度条
        self.progress_manager.close_all_progress_bars("清理中");
        
        // 关闭ASR管理器资源
        if let Err(e) = self.asr_manager.close() {
            warn!("关闭ASR管理器时出错: {}", e);
        }
        
        // 清理临时目录
        if self.temp_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&self.temp_dir) {
                warn!("清理临时目录时出错: {}", e);
            }
        }
    }
    
    /// 设置中断标志
    pub fn set_interrupt_flag(&self, value: bool) {
        let mut flag = self.interrupt_flag.lock().unwrap();
        *flag = value;
    }


}

impl ProcessingStats {
    fn new() -> Self {
        Self {
            start_time: None,
            end_time: None,
            total_files: 0,
            processed_files: 0,
            successful_files: 0,
            failed_files: 0,
            total_segments: 0,
            successful_segments: 0,
            failed_segments: 0,
        }
    }
}