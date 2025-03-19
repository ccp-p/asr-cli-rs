use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use log::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::Local;
use notify::{Watcher, RecursiveMode, Event, RecommendedWatcher};
use tokio::sync::mpsc::{self};
use tokio::task;
use tokio::time;
use anyhow::{Result, anyhow, Context};

use crate::core::audio_extractor::AudioExtractor;
use crate::core::file_utils::{load_json_file, save_json_file};
use crate::core::error::AudioToolsError;
use crate::processing::text_processor::TextProcessor;
use crate::processing::transcription_processor::TranscriptionProcessor;
use crate::processing::part_manager::PartManager;
use crate::asr::utils::get_audio_duration;

// 进度回调函数类型
type ProgressCallback = Arc<dyn Fn(usize, usize, Option<String>, Option<String>) + Send + Sync>;

/// 已处理文件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileRecord {
    last_processed_time: String,
    processed_parts: Vec<usize>,
    total_parts: usize,
    part_stats: HashMap<String, Value>,
    completed: bool,
}

impl Default for FileRecord {
    fn default() -> Self {
        Self {
            last_processed_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            processed_parts: Vec::new(),
            total_parts: 0,
            part_stats: HashMap::new(),
            completed: false,
        }
    }
}
// 继续 src/processing/file_processor.rs

/// 文件处理器，负责整体文件处理流程
pub struct FileProcessor {
    // 配置
    media_folder: PathBuf,
    output_folder: PathBuf,
    temp_segments_dir: PathBuf,
    process_video: bool,
    extract_audio_only: bool,
    format_text: bool,
    include_timestamps: bool,
    max_part_time: u32, // 单位：分钟
    max_retries: u32,
    
    // 组件
    transcription_processor: Arc<TranscriptionProcessor>,
    audio_extractor: Arc<AudioExtractor>,
    text_processor: Arc<TextProcessor>,
    
    // 回调和状态
    progress_callback: Option<ProgressCallback>,
    processed_audio: Arc<Mutex<HashMap<String, FileRecord>>>,
    processed_record_file: PathBuf,
    interrupt_flag: Arc<Mutex<bool>>,
    
    // 支持的文件类型
    video_extensions: Vec<String>,
}

impl FileProcessor {
    /// 创建新的文件处理器
    pub fn new(
        media_folder: PathBuf,
        output_folder: PathBuf,
        temp_segments_dir: &Path,
        transcription_processor: Arc<TranscriptionProcessor>,
        audio_extractor: Arc<AudioExtractor>,
        progress_callback: Option<ProgressCallback>,
        process_video: bool,
        extract_audio_only: bool,
        format_text: bool,
        include_timestamps: bool,
        max_part_time: u32,
        max_retries: u32,
    ) -> Result<Self> {
        // 创建输出目录
        fs::create_dir_all(&output_folder)?;
        
        // 设置处理记录文件路径
        let processed_record_file = output_folder.join("processed_audio_files.json");
        
        // 加载已处理文件记录
        let processed_audio = load_json_file::<HashMap<String, FileRecord>>(&processed_record_file)
            .unwrap_or_default();
        
        // 设置支持的视频文件类型
        let video_extensions = if process_video {
            vec![".mp4".to_string(), ".mov".to_string(), ".avi".to_string()]
        } else {
            Vec::new()
        };
        
        // 创建文本处理器
        let text_processor = Arc::new(TextProcessor::new(
            output_folder.clone(),
            format_text,
            include_timestamps,
            progress_callback.clone(),
        ));
        
        Ok(Self {
            media_folder,
            output_folder,
            temp_segments_dir: temp_segments_dir.to_path_buf(),
            process_video,
            extract_audio_only,
            format_text,
            include_timestamps,
            max_part_time,
            max_retries,
            transcription_processor,
            audio_extractor,
            text_processor,
            progress_callback,
            processed_audio: Arc::new(Mutex::new(processed_audio)),
            processed_record_file,
            interrupt_flag: Arc::new(Mutex::new(false)),
            video_extensions,
        })
    }
    
    /// 设置中断标志
    pub fn set_interrupt_flag(&self, value: bool) {
        let mut flag = self.interrupt_flag.lock().unwrap();
        *flag = value;
        
        // 传递给转写处理器
        self.transcription_processor.set_interrupt_flag(value);
    }
    
    /// 检查文件是否已经识别过
    fn is_recognized_file(&self, filepath: &Path) -> bool {
        // 获取不带扩展名的基本文件名
        let base_name = filepath.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        
        // 检查对应的MP3文件是否在已处理记录中
        let audio_path = self.output_folder.join(format!("{}.mp3", base_name));
        
        // 比较规范化的路径
        let processed_audio = self.processed_audio.lock().unwrap();
        for key in processed_audio.keys() {
            if Path::new(key).canonicalize().ok() == audio_path.canonicalize().ok() {
                return true;
            }
        }
        
        false
    }
    
    /// 保存处理记录
    fn save_processed_records(&self) -> Result<()> {
        let processed_audio = self.processed_audio.lock().unwrap();
        save_json_file(&self.processed_record_file, &*processed_audio)
            .context("保存处理记录失败")
    }
    
    /// 处理单个文件
    pub fn process_file(&self, filepath: &Path) -> Result<bool> {
        let filename = filepath.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("未知文件");
            
        let file_extension = filepath.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
            
        // 检查是否已处理过
        if self.is_recognized_file(filepath) {
            info!("文件已处理过: {}，跳过", filename);
            return Ok(true);
        }
        
        // 处理视频文件
        if self.video_extensions.iter().any(|ext| ext.trim_start_matches('.') == file_extension) {
            return self.process_video_file(filepath);
        }
        // 处理音频文件
        else if file_extension == "mp3" {
            return self.process_audio_file(filepath);
        }
        else {
            warn!("不支持的文件类型: {}", filename);
            return Ok(false);
        }
    }



    /// 处理视频文件
    fn process_video_file(&self, video_path: &Path) -> Result<bool> {
        let filename = video_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("未知文件");
            
        info!("处理视频文件: {}", filename);
        
        // 提取音频
        let (audio_path, is_new) = self.audio_extractor.extract_audio_from_video(
            video_path, 
            &self.output_folder
        )?;
        
        if audio_path.is_none() {
            error!("从视频提取音频失败: {}", filename);
            return Ok(false);
        }
        
        let audio_path = audio_path.unwrap();
        
        // 如果只需要提取音频，到此为止
        if self.extract_audio_only {
            if is_new {
                info!("已提取音频: {}", audio_path.display());
            } else {
                info!("已存在音频: {}", audio_path.display());
            }
            return Ok(true);
        }
        
        // 继续处理提取出的音频文件
        self.process_audio_file(&audio_path)
    }
    
    /// 处理音频文件
    fn process_audio_file(&self, audio_path: &Path) -> Result<bool> {
        let filename = audio_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("未知文件");
            
        info!("处理音频文件: {}", filename);
        
        // 获取音频时长
        let audio_duration = get_audio_duration(audio_path)?;
        if audio_duration <= 0.0 {
            error!("无法获取音频时长: {}", filename);
            return Ok(false);
        }
        
        info!("音频时长: {:.1}秒", audio_duration);
        
        // 判断是否为大音频文件（超过设置的分钟数）
        if audio_duration > (self.max_part_time as f64 * 60.0) {
            return self.process_large_audio_file(audio_path, audio_duration);
        }
        
        // 处理正常大小的音频文件
        let segment_files = self.audio_extractor.split_audio_file(audio_path)?;
        if segment_files.is_empty() {
            error!("分割音频失败: {}", filename);
            return Ok(false);
        }
        
        // 处理音频片段
        let segment_results = self.transcription_processor.process_audio_segments(&segment_files)?;
        
        // 重试失败的片段
        let segment_results = if !segment_results.is_empty() {
            self.transcription_processor.retry_failed_segments(&segment_files, segment_results)?
        } else {
            HashMap::new()
        };
        
        // 处理转写结果，生成文本文件
        if let Some(callback) = &self.progress_callback {
            callback(0, 1, Some("准备生成文本文件...".to_string()), None);
        }
        
        // 准备元数据
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let metadata = HashMap::from([
            ("原始文件".to_string(), Value::String(filename.to_string())),
            ("处理时间".to_string(), Value::String(current_time)),
            ("识别成功率".to_string(), Value::String(format!("{}/{} 片段", segment_results.len(), segment_files.len()))),
            ("音频长度".to_string(), Value::String(format!("{}秒", segment_files.len() * 30))),
        ]);
        
        // 准备文本内容
        let result_text = self.text_processor.prepare_result_text(
            &segment_files,
            &segment_results,
            Some(&metadata)
        )?;
        
        if result_text.is_empty() {
            warn!("无有效转写结果: {}", filename);
            return Ok(false);
        }
        
        // 保存文本文件
        let output_file = self.text_processor.save_result_text(
            &result_text,
            filename,
            None
        )?;
        
        if let Some(callback) = &self.progress_callback {
            callback(
                1, 
                1, 
                Some(format!("文本生成完成: {}", output_file.file_name().unwrap_or_default().to_string_lossy())),
                None
            );
        }
        
        info!("转写结果已保存到: {}", output_file.display());
        
        // 更新处理记录
        {
            let mut processed_audio = self.processed_audio.lock().unwrap();
            let audio_path_str = audio_path.to_string_lossy().to_string();
            
            if !processed_audio.contains_key(&audio_path_str) {
                processed_audio.insert(audio_path_str.clone(), FileRecord::default());
            }
            
            if let Some(record) = processed_audio.get_mut(&audio_path_str) {
                record.last_processed_time = current_time;
            }
        }
        
        // 保存处理记录
        self.save_processed_records()?;
        
        // 删除音频文件
        if audio_path.exists() {
            fs::remove_file(audio_path)?;
            info!("删除音频文件: {}", audio_path.display());
        }
        
        Ok(true)
    }


    /// 处理大音频文件
    fn process_large_audio_file(&self, audio_path: &Path, audio_duration: f64) -> Result<bool> {
        let filename = audio_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("未知文件");
            
        info!("检测到大音频文件: {}，长度: {:.1}分钟，开始分part处理",
              filename, audio_duration / 60.0);
              
        // 创建Part管理器
        let part_manager = PartManager::new(&self.output_folder);
        
        // 获取part信息和待处理part
        let (file_record, pending_parts) = {
            let mut processed_audio = self.processed_audio.lock().unwrap();
            part_manager.get_parts_for_audio(
                audio_path, 
                audio_duration, 
                &mut *processed_audio
            )
        };
        
        // 如果所有part都已完成，创建索引文件并返回
        if pending_parts.is_empty() {
            info!("音频 {} 所有part已处理完成", filename);
            
            // 创建索引文件
            {
                let processed_audio = self.processed_audio.lock().unwrap();
                let index_file = part_manager.create_index_file(audio_path, &*processed_audio)?;
                info!("创建索引文件: {}", index_file.display());
            }
            
            self.save_processed_records()?;
            return Ok(true);
        }
        
        // 分割音频为片段
        let segment_files = self.audio_extractor.split_audio_file(audio_path)?;
        if segment_files.is_empty() {
            error!("分割音频失败: {}", filename);
            return Ok(false);
        }
        
        // 依次处理每个pending的part
        let total_pending = pending_parts.len();
        for (i, part_idx) in pending_parts.iter().enumerate() {
            // 检查中断标志
            if *self.interrupt_flag.lock().unwrap() {
                warn!("处理被中断，已完成 {}/{} 个待处理part", i, total_pending);
                break;
            }
            
            // 获取这个part的片段文件
            let part_segments = part_manager.get_segments_for_part(
                *part_idx, 
                &segment_files
            );
            
            info!("处理Part {}/{}，包含 {} 个片段", 
                 part_idx + 1, 
                 file_record.total_parts,
                 part_segments.len());
                 
            // 显示进度
            if let Some(callback) = &self.progress_callback {
                callback(
                    i,
                    total_pending,
                    Some(format!("处理Part {}/{}", part_idx + 1, file_record.total_parts)),
                    None
                );
            }
            
            // 处理这个part的所有片段
            let segment_results = self.transcription_processor.process_audio_segments(&part_segments)?;
            
            // 重试失败的片段
            let segment_results = if !segment_results.is_empty() {
                self.transcription_processor.retry_failed_segments(&part_segments, segment_results)?
            } else {
                HashMap::new()
            };
            
            // 准备part的文本内容
            let (start_time, end_time) = part_manager.get_part_time_range(*part_idx);
            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let part_metadata = HashMap::from([
                ("原始文件".to_string(), Value::String(filename.to_string())),
                ("Part编号".to_string(), Value::String(format!("{}/{}", part_idx + 1, file_record.total_parts))),
                ("时间范围".to_string(), Value::String(format!("{:.1}-{:.1}分钟", 
                                            start_time / 60.0, 
                                            (end_time.min(audio_duration)) / 60.0))),
                ("处理时间".to_string(), Value::String(current_time)),
            ]);
            
            let part_text = self.text_processor.prepare_result_text(
                &part_segments,
                &segment_results,
                Some(&part_metadata)
            )?;
            
            // 保存part的文本
            if !part_text.is_empty() {
                let output_file = {
                    let mut processed_audio = self.processed_audio.lock().unwrap();
                    part_manager.save_part_text(
                        audio_path, 
                        *part_idx, 
                        &part_text, 
                        &mut *processed_audio
                    )?
                };
                
                info!("Part {} 转写结果已保存: {}", part_idx + 1, output_file.display());
                
                // 保存进度
                self.save_processed_records()?;
            } else {
                warn!("Part {} 无有效转写结果", part_idx + 1);
            }
        }
        
        // 检查是否全部完成
        let is_completed = {
            let processed_audio = self.processed_audio.lock().unwrap();
            let file_record = processed_audio.get(&audio_path.to_string_lossy().to_string())
                .cloned()
                .unwrap_or_default();
                
            file_record.completed
        };
        
        if is_completed {
            // 创建索引文件
            {
                let processed_audio = self.processed_audio.lock().unwrap();
                let index_file = part_manager.create_index_file(audio_path, &*processed_audio)?;
                info!("所有Part处理完成，创建索引文件: {}", index_file.display());
            }
        }
        
        // 保存最终状态
        self.save_processed_records()?;
        
        Ok(true)
    }
}




// 继续 src/processing/file_processor.rs

// 文件事件处理结构，替代Python中的AudioFileHandler
pub struct FileWatcher {
    processor: Arc<FileProcessor>,
    media_folder: PathBuf,
    audio_extensions: Vec<String>,
    processed_files: Arc<Mutex<HashSet<PathBuf>>>,
    pending_files: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    debounce_seconds: u64,
    watcher: Option<RecommendedWatcher>,
    watcher_running: Arc<Mutex<bool>>,
}

impl FileWatcher {
    // 创建新的文件监控器
    fn new(processor: Arc<FileProcessor>, debounce_seconds: u64) -> Self {
        let audio_extensions = vec![
            ".mp3".to_string(), ".wav".to_string(), ".m4a".to_string(), 
            ".flac".to_string(), ".ogg".to_string(), ".aac".to_string(),
        ];
        
        // 添加视频扩展名
        let mut all_extensions = audio_extensions.clone();
        if processor.process_video {
            all_extensions.extend(processor.video_extensions.clone());
        }
        
        Self {
            processor: processor.clone(),
            media_folder: processor.media_folder.clone(),
            audio_extensions: all_extensions,
            processed_files: Arc::new(Mutex::new(HashSet::new())),
            pending_files: Arc::new(Mutex::new(HashMap::new())),
            debounce_seconds,
            watcher: None,
            watcher_running: Arc::new(Mutex::new(false)),
        }
    }
    
    // 检查文件是否为支持的媒体文件
    fn is_media_file(&self, path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }
        
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext = format!(".{}", ext.to_lowercase());
                self.audio_extensions.contains(&ext)
            })
            .unwrap_or(false)
    }
    
    // 处理文件事件
    async fn handle_file_event(&self, path: PathBuf) {
        // 如果不是媒体文件或已在处理队列中，则跳过
        if !self.is_media_file(&path) {
            return;
        }
        
        // 文件路径字符串，用于日志
        let path_str = path.to_string_lossy();
        
        // 检查是否已处理过
        {
            let processed_files = self.processed_files.lock().unwrap();
            if processed_files.contains(&path) {
                debug!("文件已在处理队列中，跳过: {}", path_str);
                return;
            }
        }
        
        // 更新待处理文件列表，实现防抖动
        {
            let mut pending_files = self.pending_files.lock().unwrap();
            pending_files.insert(path.clone(), Instant::now());
        }
        
        debug!("文件事件触发，设置处理延时: {}", path_str);
        
        // 克隆Arc引用以在异步任务中使用
        let processor = self.processor.clone();
        let pending_files = self.pending_files.clone();
        let processed_files = self.processed_files.clone();
        let debounce_seconds = self.debounce_seconds;
        
        // 创建异步任务处理文件
        tokio::spawn(async move {
            // 等待防抖动延迟
            time::sleep(Duration::from_secs(debounce_seconds)).await;
            
            // 检查文件是否仍然存在
            if !path.exists() {
                // 从待处理列表中移除
                let mut pending_files = pending_files.lock().unwrap();
                pending_files.remove(&path);
                return;
            }
            
            // 确认文件仍在待处理列表中
            {
                let pending_time = {
                    let pending_files = pending_files.lock().unwrap();
                    pending_files.get(&path).cloned()
                };
                
                // 如果不在待处理列表或时间太短，跳过
                if let Some(time) = pending_time {
                    let elapsed = time.elapsed();
                    if elapsed < Duration::from_secs(debounce_seconds) {
                        return; // 还有另一个更新的事件等待处理
                    }
                } else {
                    return; // 已被其他任务移除
                }
            }
            
            // 从待处理移除，添加到处理中
            {
                let mut pending_files = pending_files.lock().unwrap();
                pending_files.remove(&path);
                
                let mut processed_files = processed_files.lock().unwrap();
                processed_files.insert(path.clone());
            }
            
            // 等待文件写入完成的额外延迟
            time::sleep(Duration::from_secs(2)).await;
            
            // 检查文件是否仍然存在
            if !path.exists() {
                let mut processed_files = processed_files.lock().unwrap();
                processed_files.remove(&path);
                return;
            }
            
            // 处理文件
            info!("开始处理文件: {}", path.to_string_lossy());
            
            match processor.process_file(&path) {
                Ok(success) => {
                    if success {
                        info!("文件处理成功: {}", path.to_string_lossy());
                    } else {
                        warn!("文件处理失败: {}", path.to_string_lossy());
                    }
                },
                Err(e) => {
                    error!("处理文件时出错 {}: {}", path.to_string_lossy(), e);
                }
            }
            
            // 处理完成，从处理列表中移除
            let mut processed_files = processed_files.lock().unwrap();
            processed_files.remove(&path);
        });
    }
    
    // 启动文件监控
    async fn start(&mut self) -> Result<()> {
        // 设置已经运行标志
        {
            let mut running = self.watcher_running.lock().unwrap();
            if *running {
                return Err(anyhow!("文件监控器已经在运行"));
            }
            *running = true;
        }
        
        // 创建事件通道
        let (tx, mut rx) = mpsc::channel(100);
        
        // 创建和配置监控器
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            let tx = tx.clone();
            
            match res {
                Ok(event) => {
                    if event.kind.is_create() || event.kind.is_modify() {
                        // 获取路径并发送到通道
                        for path in event.paths {
                            let _ = tx.try_send(path);
                        }
                    }
                },
                Err(e) => error!("监控错误: {:?}", e),
            }
        })?;
        
        // 开始监控目录
        watcher.watch(&self.media_folder, RecursiveMode::NonRecursive)?;
        info!("开始监控目录: {}", self.media_folder.display());
        
        // 保存监控器实例
        self.watcher = Some(watcher);
        
        // 克隆自身引用用于异步任务
        let self_ref = Arc::new(self);
        
        // 异步任务监听事件
        task::spawn(async move {
            while let Some(path) = rx.recv().await {
                self_ref.handle_file_event(path).await;
            }
        });
        
        Ok(())
    }
    
    // 停止文件监控
    fn stop(&mut self) -> Result<()> {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            
            // 更新运行状态
            let mut running = self.watcher_running.lock().unwrap();
            *running = false;
            
            info!("已停止文件监控");
        }
        
        Ok(())
    }
}

impl FileProcessor {
    // 启动文件监控
    pub async fn start_file_monitoring(&self) -> Result<FileWatcher> {
        let processor = Arc::new(self.clone());
        let mut watcher = FileWatcher::new(processor, 5);
        watcher.start().await?;
        Ok(watcher)
    }
    
    // 为了支持克隆
    pub fn clone(&self) -> Self {
        Self {
            media_folder: self.media_folder.clone(),
            output_folder: self.output_folder.clone(),
            temp_segments_dir: self.temp_segments_dir.clone(),
            process_video: self.process_video,
            extract_audio_only: self.extract_audio_only,
            format_text: self.format_text,
            include_timestamps: self.include_timestamps,
            max_part_time: self.max_part_time,
            max_retries: self.max_retries,
            transcription_processor: Arc::clone(&self.transcription_processor),
            audio_extractor: Arc::clone(&self.audio_extractor),
            text_processor: Arc::clone(&self.text_processor),
            progress_callback: self.progress_callback.clone(),
            processed_audio: Arc::clone(&self.processed_audio),
            processed_record_file: self.processed_record_file.clone(),
            interrupt_flag: Arc::clone(&self.interrupt_flag),
            video_extensions: self.video_extensions.clone(),
        }
    }
}