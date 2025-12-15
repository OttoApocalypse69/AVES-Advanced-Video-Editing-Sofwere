//! Media decoding subsystem using FFmpeg.
//! Per SPEC.md: Video decode output is RGBA8, Audio decode output is interleaved PCM f32.
//! All unsafe FFmpeg code is isolated in this module.

use std::path::{Path, PathBuf};
use std::ffi::CString;
use std::sync::Arc;
use crossbeam::channel;
use crate::core::time::Time;

/// Error type for decoding operations
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("FFmpeg error: {0}")]
    FFmpeg(String),
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("No video stream found")]
    NoVideoStream,
    #[error("No audio stream found")]
    NoAudioStream,
    #[error("Invalid stream index: {0}")]
    InvalidStreamIndex(usize),
    #[error("Seek failed: {0}")]
    SeekFailed(String),
    #[error("Codec not found")]
    CodecNotFound,
    #[error("Failed to open codec")]
    CodecOpenFailed,
}

/// Decoded video frame (RGBA8 as per SPEC.md)
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,      // Raw pixel data (RGBA8)
    pub width: u32,
    pub height: u32,
    pub timestamp: Time,    // Timestamp in nanoseconds
}

/// Decoded audio frame (interleaved PCM f32 as per SPEC.md)
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<f32>,     // Interleaved PCM samples (L, R, L, R, ...)
    pub sample_rate: u32,
    pub channels: u32,
    pub timestamp: Time,    // Timestamp in nanoseconds
}

/// Stream information
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub index: usize,
    pub codec_name: String,
    pub duration: Time,  // Duration in nanoseconds
    pub timebase_num: i32,
    pub timebase_den: i32,
}

/// Video stream information
#[derive(Debug, Clone)]
pub struct VideoStreamInfo {
    pub stream_info: StreamInfo,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub pixel_format: String,
}

/// Audio stream information
#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    pub stream_info: StreamInfo,
    pub sample_rate: u32,
    pub channels: u32,
    pub sample_format: String,
}

/// Media decoder with FFmpeg backend
/// All unsafe FFmpeg operations are isolated within this struct
pub struct MediaDecoder {
    path: PathBuf,
    // FFmpeg contexts (opaque pointers, accessed only via unsafe blocks)
    inner: Arc<FFmpegContext>,
}

/// Internal FFmpeg context (unsafe implementation details)
/// This struct contains all FFmpeg pointers and is only accessed via unsafe blocks
struct FFmpegContext {
    format_ctx: *mut ffmpeg_next::ffi::AVFormatContext,
    video_codec_ctx: Option<*mut ffmpeg_next::ffi::AVCodecContext>,
    audio_codec_ctx: Option<*mut ffmpeg_next::ffi::AVCodecContext>,
    video_stream_index: Option<usize>,
    audio_stream_index: Option<usize>,
    sws_ctx: Option<*mut ffmpeg_next::ffi::SwsContext>,  // For video scaling/conversion to RGBA8
    swr_ctx: Option<*mut ffmpeg_next::ffi::SwrContext>, // For audio resampling/conversion to f32
    video_timebase: Option<(i32, i32)>,  // (num, den) for timestamp conversion
    audio_timebase: Option<(i32, i32)>,  // (num, den) for timestamp conversion
}

unsafe impl Send for FFmpegContext {}
unsafe impl Sync for FFmpegContext {}

impl MediaDecoder {
    /// Create a new media decoder for a file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DecodeError> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(DecodeError::FileNotFound(path.to_path_buf()));
        }

        // All FFmpeg operations are unsafe
        let inner = unsafe { Self::open_ffmpeg_context(path)? };
        
        Ok(Self {
            path: path.to_path_buf(),
            inner: Arc::new(inner),
        })
    }

    /// Open FFmpeg context (unsafe block)
    /// This function contains all unsafe FFmpeg API calls
    unsafe fn open_ffmpeg_context(path: &Path) -> Result<FFmpegContext, DecodeError> {
        use ffmpeg_next as ffmpeg;
        
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| DecodeError::FFmpeg(format!("FFmpeg init failed: {:?}", e)))?;

        // Open input file
        let path_cstr = CString::new(path.to_string_lossy().as_ref())
            .map_err(|e| DecodeError::FFmpeg(format!("Invalid path: {}", e)))?;
        
        let mut format_ctx: *mut ffmpeg::ffi::AVFormatContext = std::ptr::null_mut();
        let format_ctx_ptr = &mut format_ctx;
        
        let ret = ffmpeg::ffi::avformat_open_input(
            format_ctx_ptr,
            path_cstr.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
        );
        
        if ret < 0 {
            return Err(DecodeError::FFmpeg(format!("Failed to open input file: {}", ret)));
        }

        // Find stream info
        let ret = ffmpeg::ffi::avformat_find_stream_info(*format_ctx_ptr, std::ptr::null_mut());
        if ret < 0 {
            ffmpeg::ffi::avformat_close_input(format_ctx_ptr);
            return Err(DecodeError::FFmpeg(format!("Failed to find stream info: {}", ret)));
        }

        let format_ctx = *format_ctx_ptr;
        
        // Find video and audio streams
        let mut video_stream_index = None;
        let mut audio_stream_index = None;
        let mut video_codec_ctx = None;
        let mut audio_codec_ctx = None;
        let mut video_timebase = None;
        let mut audio_timebase = None;

        let nb_streams = (*format_ctx).nb_streams;
        
        for i in 0..nb_streams {
            let stream = *(*format_ctx).streams.add(i as usize);
            let codecpar = (*stream).codecpar;
            let codec_type = (*codecpar).codec_type;

            if codec_type == ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO && video_stream_index.is_none() {
                video_stream_index = Some(i as usize);
                
                // Get codec
                let codec_id = (*codecpar).codec_id;
                let codec = ffmpeg::ffi::avcodec_find_decoder(codec_id);
                if codec.is_null() {
                    continue;
                }

                // Allocate codec context
                let codec_ctx = ffmpeg::ffi::avcodec_alloc_context3(codec);
                if codec_ctx.is_null() {
                    continue;
                }

                // Copy codec parameters
                let ret = ffmpeg::ffi::avcodec_parameters_to_context(codec_ctx, codecpar);
                if ret < 0 {
                    ffmpeg::ffi::avcodec_free_context(&mut codec_ctx);
                    continue;
                }

                // Open codec
                let ret = ffmpeg::ffi::avcodec_open2(codec_ctx, codec, std::ptr::null_mut());
                if ret < 0 {
                    ffmpeg::ffi::avcodec_free_context(&mut codec_ctx);
                    continue;
                }

                video_codec_ctx = Some(codec_ctx);
                
                // Get timebase
                let time_base = (*stream).time_base;
                video_timebase = Some((time_base.num, time_base.den));
            } else if codec_type == ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO && audio_stream_index.is_none() {
                audio_stream_index = Some(i as usize);
                
                // Get codec
                let codec_id = (*codecpar).codec_id;
                let codec = ffmpeg::ffi::avcodec_find_decoder(codec_id);
                if codec.is_null() {
                    continue;
                }

                // Allocate codec context
                let codec_ctx = ffmpeg::ffi::avcodec_alloc_context3(codec);
                if codec_ctx.is_null() {
                    continue;
                }

                // Copy codec parameters
                let ret = ffmpeg::ffi::avcodec_parameters_to_context(codec_ctx, codecpar);
                if ret < 0 {
                    ffmpeg::ffi::avcodec_free_context(&mut codec_ctx);
                    continue;
                }

                // Open codec
                let ret = ffmpeg::ffi::avcodec_open2(codec_ctx, codec, std::ptr::null_mut());
                if ret < 0 {
                    ffmpeg::ffi::avcodec_free_context(&mut codec_ctx);
                    continue;
                }

                audio_codec_ctx = Some(codec_ctx);
                
                // Get timebase
                let time_base = (*stream).time_base;
                audio_timebase = Some((time_base.num, time_base.den));
            }
        }

        // Initialize SwsContext for video conversion to RGBA8
        let sws_ctx = if let Some(codec_ctx) = video_codec_ctx {
            let width = (*codec_ctx).width;
            let height = (*codec_ctx).height;
            let pix_fmt = (*codec_ctx).pix_fmt;
            
            let sws = ffmpeg::ffi::sws_getContext(
                width,
                height,
                pix_fmt,
                width,
                height,
                ffmpeg::ffi::AVPixelFormat::AV_PIX_FMT_RGBA,
                ffmpeg::ffi::SwsFlags::SWS_BILINEAR,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            
            if sws.is_null() {
                None
            } else {
                Some(sws)
            }
        } else {
            None
        };

        // Initialize SwrContext for audio conversion to f32 interleaved
        let swr_ctx = if let Some(codec_ctx) = audio_codec_ctx {
            let sample_fmt = (*codec_ctx).sample_fmt;
            let sample_rate = (*codec_ctx).sample_rate;
            let channels = (*codec_ctx).ch_layout.nb_channels;
            
            let swr = ffmpeg::ffi::swr_alloc();
            if swr.is_null() {
                None
            } else {
                // Set input parameters
                ffmpeg::ffi::av_opt_set_chlayout(swr, b"in_chlayout\0".as_ptr() as *const i8, &(*codec_ctx).ch_layout, 0);
                ffmpeg::ffi::av_opt_set_int(swr, b"in_sample_rate\0".as_ptr() as *const i8, sample_rate as i64, 0);
                ffmpeg::ffi::av_opt_set_sample_fmt(swr, b"in_sample_fmt\0".as_ptr() as *const i8, sample_fmt, 0);
                
                // Set output parameters (f32 interleaved - AV_SAMPLE_FMT_FLT)
                let mut out_ch_layout = ffmpeg::ffi::AVChannelLayout::default();
                ffmpeg::ffi::av_channel_layout_copy(&mut out_ch_layout, &(*codec_ctx).ch_layout);
                ffmpeg::ffi::av_opt_set_chlayout(swr, b"out_chlayout\0".as_ptr() as *const i8, &out_ch_layout, 0);
                ffmpeg::ffi::av_opt_set_int(swr, b"out_sample_rate\0".as_ptr() as *const i8, sample_rate as i64, 0);
                // AV_SAMPLE_FMT_FLT is f32 interleaved (per SPEC.md)
                ffmpeg::ffi::av_opt_set_sample_fmt(swr, b"out_sample_fmt\0".as_ptr() as *const i8, ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT, 0);
                
                let ret = ffmpeg::ffi::swr_init(swr);
                if ret < 0 {
                    ffmpeg::ffi::swr_free(&mut swr);
                    None
                } else {
                    Some(swr)
                }
            }
        } else {
            None
        };

        Ok(FFmpegContext {
            format_ctx,
            video_codec_ctx,
            audio_codec_ctx,
            video_stream_index,
            audio_stream_index,
            sws_ctx,
            swr_ctx,
            video_timebase,
            audio_timebase,
        })
    }

    /// Get video stream information
    pub fn get_video_stream_info(&self) -> Result<VideoStreamInfo, DecodeError> {
        unsafe {
            let ctx = &*self.inner;
            
            let stream_index = ctx.video_stream_index
                .ok_or(DecodeError::NoVideoStream)?;
            let codec_ctx = ctx.video_codec_ctx
                .ok_or(DecodeError::NoVideoStream)?;
            let format_ctx = ctx.format_ctx;
            
            let stream = *(*format_ctx).streams.add(stream_index);
            let duration = (*stream).duration;
            let time_base = (*stream).time_base;
            let duration_ns = Self::ffmpeg_time_to_nanos(duration, time_base.num, time_base.den);
            
            let codec_name = {
                let codec = (*codec_ctx).codec_id;
                let codec_name_cstr = ffmpeg_next::ffi::avcodec_get_name(codec);
                if codec_name_cstr.is_null() {
                    "unknown".to_string()
                } else {
                    std::ffi::CStr::from_ptr(codec_name_cstr)
                        .to_string_lossy()
                        .to_string()
                }
            };

            let width = (*codec_ctx).width as u32;
            let height = (*codec_ctx).height as u32;
            
            // Calculate FPS
            let fps = {
                let r_frame_rate = (*stream).r_frame_rate;
                if r_frame_rate.den != 0 {
                    r_frame_rate.num as f64 / r_frame_rate.den as f64
                } else {
                    30.0  // Default
                }
            };

            let pix_fmt = (*codec_ctx).pix_fmt;
            let pixel_format = format!("{:?}", pix_fmt);

            Ok(VideoStreamInfo {
                stream_info: StreamInfo {
                    index: stream_index,
                    codec_name,
                    duration: duration_ns,
                    timebase_num: time_base.num,
                    timebase_den: time_base.den,
                },
                width,
                height,
                fps,
                pixel_format,
            })
        }
    }

    /// Get audio stream information
    pub fn get_audio_stream_info(&self) -> Result<AudioStreamInfo, DecodeError> {
        unsafe {
            let ctx = &*self.inner;
            
            let stream_index = ctx.audio_stream_index
                .ok_or(DecodeError::NoAudioStream)?;
            let codec_ctx = ctx.audio_codec_ctx
                .ok_or(DecodeError::NoAudioStream)?;
            let format_ctx = ctx.format_ctx;
            
            let stream = *(*format_ctx).streams.add(stream_index);
            let duration = (*stream).duration;
            let time_base = (*stream).time_base;
            let duration_ns = Self::ffmpeg_time_to_nanos(duration, time_base.num, time_base.den);
            
            let codec_name = {
                let codec = (*codec_ctx).codec_id;
                let codec_name_cstr = ffmpeg_next::ffi::avcodec_get_name(codec);
                if codec_name_cstr.is_null() {
                    "unknown".to_string()
                } else {
                    std::ffi::CStr::from_ptr(codec_name_cstr)
                        .to_string_lossy()
                        .to_string()
                }
            };

            let sample_rate = (*codec_ctx).sample_rate as u32;
            let channels = (*codec_ctx).ch_layout.nb_channels as u32;
            let sample_fmt = (*codec_ctx).sample_fmt;
            let sample_format = format!("{:?}", sample_fmt);

            Ok(AudioStreamInfo {
                stream_info: StreamInfo {
                    index: stream_index,
                    codec_name,
                    duration: duration_ns,
                    timebase_num: time_base.num,
                    timebase_den: time_base.den,
                },
                sample_rate,
                channels,
                sample_format,
            })
        }
    }

    /// Find video stream index
    pub fn find_video_stream(&self) -> Result<usize, DecodeError> {
        self.inner.video_stream_index
            .ok_or(DecodeError::NoVideoStream)
    }

    /// Find audio stream index
    pub fn find_audio_stream(&self) -> Result<usize, DecodeError> {
        self.inner.audio_stream_index
            .ok_or(DecodeError::NoAudioStream)
    }

    /// Seek to nearest keyframe at or before the specified timestamp (nanoseconds)
    pub fn seek(&self, timestamp: Time, stream_index: usize) -> Result<(), DecodeError> {
        unsafe {
            let ctx = &*self.inner;
            let format_ctx = ctx.format_ctx;
            
            // Get the stream
            let stream = *(*format_ctx).streams.add(stream_index);
            let time_base = (*stream).time_base;
            
            // Convert nanoseconds to FFmpeg timebase
            let target_pts = Self::nanos_to_ffmpeg_time(timestamp, time_base.num, time_base.den);
            
            // Seek to nearest keyframe (AVSEEK_FLAG_BACKWARD)
            let ret = ffmpeg_next::ffi::av_seek_frame(
                format_ctx,
                stream_index as i32,
                target_pts,
                ffmpeg_next::ffi::AVSEEK_FLAG_BACKWARD,
            );
            
            if ret < 0 {
                return Err(DecodeError::SeekFailed(format!("av_seek_frame returned {}", ret)));
            }

            // Flush codec buffers
            if let Some(codec_ctx) = ctx.video_codec_ctx {
                ffmpeg_next::ffi::avcodec_flush_buffers(codec_ctx);
            }
            if let Some(codec_ctx) = ctx.audio_codec_ctx {
                ffmpeg_next::ffi::avcodec_flush_buffers(codec_ctx);
            }

            Ok(())
        }
    }

    /// Start decoding video frames and send them via channel
    /// Returns a receiver that will receive VideoFrame messages
    pub fn start_video_decoding(&self, stream_index: usize) -> Result<channel::Receiver<VideoFrame>, DecodeError> {
        let (tx, rx) = channel::unbounded();
        let inner = Arc::clone(&self.inner);
        
        std::thread::spawn(move || {
            let _ = Self::decode_video_loop(inner, stream_index, tx);
        });

        Ok(rx)
    }

    /// Start decoding audio frames and send them via channel
    /// Returns a receiver that will receive AudioFrame messages
    pub fn start_audio_decoding(&self, stream_index: usize) -> Result<channel::Receiver<AudioFrame>, DecodeError> {
        let (tx, rx) = channel::unbounded();
        let inner = Arc::clone(&self.inner);
        
        std::thread::spawn(move || {
            let _ = Self::decode_audio_loop(inner, stream_index, tx);
        });

        Ok(rx)
    }

    /// Video decoding loop (runs in separate thread)
    fn decode_video_loop(
        inner: Arc<FFmpegContext>,
        stream_index: usize,
        tx: channel::Sender<VideoFrame>,
    ) -> Result<(), DecodeError> {
        unsafe {
            let ctx = &*inner;
            let format_ctx = ctx.format_ctx;
            let codec_ctx = ctx.video_codec_ctx
                .ok_or(DecodeError::NoVideoStream)?;
            let sws_ctx = ctx.sws_ctx
                .ok_or(DecodeError::NoVideoStream)?;
            let timebase = ctx.video_timebase
                .ok_or(DecodeError::NoVideoStream)?;

            let width = (*codec_ctx).width as u32;
            let height = (*codec_ctx).height as u32;

            let mut packet = ffmpeg_next::ffi::av_packet_alloc();
            let mut frame = ffmpeg_next::ffi::av_frame_alloc();
            let mut rgb_frame = ffmpeg_next::ffi::av_frame_alloc();

            if packet.is_null() || frame.is_null() || rgb_frame.is_null() {
                return Err(DecodeError::FFmpeg("Failed to allocate frame/packet".to_string()));
            }

            // Allocate buffer for RGBA8 frame
            let num_bytes = ffmpeg_next::ffi::av_image_get_buffer_size(
                ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_RGBA,
                width as i32,
                height as i32,
                1,
            );
            let mut buffer = vec![0u8; num_bytes as usize];
            let buffer_ptr = buffer.as_mut_ptr();

            let ret = ffmpeg_next::ffi::av_image_fill_arrays(
                (*rgb_frame).data.as_mut_ptr(),
                (*rgb_frame).linesize.as_mut_ptr(),
                buffer_ptr,
                ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_RGBA,
                width as i32,
                height as i32,
                1,
            );

            if ret < 0 {
                return Err(DecodeError::FFmpeg("Failed to fill image arrays".to_string()));
            }

            loop {
                // Read packet
                let ret = ffmpeg_next::ffi::av_read_frame(format_ctx, packet);
                if ret < 0 {
                    break;  // EOF or error
                }

                if (*packet).stream_index != stream_index as i32 {
                    ffmpeg_next::ffi::av_packet_unref(packet);
                    continue;
                }

                // Send packet to decoder
                let ret = ffmpeg_next::ffi::avcodec_send_packet(codec_ctx, packet);
                ffmpeg_next::ffi::av_packet_unref(packet);
                
                if ret < 0 {
                    continue;
                }

                // Receive frames
                loop {
                    let ret = ffmpeg_next::ffi::avcodec_receive_frame(codec_ctx, frame);
                    if ret < 0 {
                        break;
                    }

                    // Convert to RGBA8
                    ffmpeg_next::ffi::sws_scale(
                        sws_ctx,
                        (*frame).data.as_ptr(),
                        (*frame).linesize.as_ptr(),
                        0,
                        height as i32,
                        (*rgb_frame).data.as_mut_ptr(),
                        (*rgb_frame).linesize.as_mut_ptr(),
                    );

                    // Convert timestamp to nanoseconds
                    let pts = (*frame).pts;
                    let timestamp = Self::ffmpeg_time_to_nanos(pts, timebase.0, timebase.1);

                    // Extract RGBA8 data
                    let linesize = (*rgb_frame).linesize[0] as usize;
                    let mut data = vec![0u8; (linesize * height as usize)];
                    for y in 0..height as usize {
                        let src = (*rgb_frame).data[0].add(y * linesize);
                        let dst = data.as_mut_ptr().add(y * linesize);
                        std::ptr::copy_nonoverlapping(src, dst, linesize);
                    }

                    let video_frame = VideoFrame {
                        data,
                        width,
                        height,
                        timestamp,
                    };

                    if tx.send(video_frame).is_err() {
                        return Ok(());  // Receiver dropped
                    }
                }
            }

            Ok(())
        }
    }

    /// Audio decoding loop (runs in separate thread)
    fn decode_audio_loop(
        inner: Arc<FFmpegContext>,
        stream_index: usize,
        tx: channel::Sender<AudioFrame>,
    ) -> Result<(), DecodeError> {
        unsafe {
            let ctx = &*inner;
            let format_ctx = ctx.format_ctx;
            let codec_ctx = ctx.audio_codec_ctx
                .ok_or(DecodeError::NoAudioStream)?;
            let swr_ctx = ctx.swr_ctx
                .ok_or(DecodeError::NoAudioStream)?;
            let timebase = ctx.audio_timebase
                .ok_or(DecodeError::NoAudioStream)?;

            let sample_rate = (*codec_ctx).sample_rate as u32;
            let channels = (*codec_ctx).ch_layout.nb_channels as u32;

            let mut packet = ffmpeg_next::ffi::av_packet_alloc();
            let mut frame = ffmpeg_next::ffi::av_frame_alloc();

            if packet.is_null() || frame.is_null() {
                return Err(DecodeError::FFmpeg("Failed to allocate frame/packet".to_string()));
            }

            loop {
                // Read packet
                let ret = ffmpeg_next::ffi::av_read_frame(format_ctx, packet);
                if ret < 0 {
                    break;  // EOF or error
                }

                if (*packet).stream_index != stream_index as i32 {
                    ffmpeg_next::ffi::av_packet_unref(packet);
                    continue;
                }

                // Send packet to decoder
                let ret = ffmpeg_next::ffi::avcodec_send_packet(codec_ctx, packet);
                ffmpeg_next::ffi::av_packet_unref(packet);
                
                if ret < 0 {
                    continue;
                }

                // Receive frames
                loop {
                    let ret = ffmpeg_next::ffi::avcodec_receive_frame(codec_ctx, frame);
                    if ret < 0 {
                        break;
                    }

                    // Convert to f32 interleaved using swresample
                    let nb_samples = (*frame).nb_samples as usize;
                    // Allocate enough space for output (may need more due to resampling)
                    let max_out_samples = (nb_samples as f64 * 1.5) as usize;
                    let mut out_samples = vec![0f32; max_out_samples * channels as usize];
                    
                    // For interleaved format (AV_SAMPLE_FMT_FLT), we need array of pointers
                    // where each pointer points to the interleaved buffer
                    let mut out_planes: [*mut u8; 8] = [std::ptr::null_mut(); 8];
                    out_planes[0] = out_samples.as_mut_ptr() as *mut u8;

                    let ret = ffmpeg_next::ffi::swr_convert(
                        swr_ctx,
                        out_planes.as_mut_ptr(),
                        max_out_samples as i32,
                        (*frame).data.as_ptr(),
                        nb_samples as i32,
                    );

                    if ret < 0 {
                        continue;
                    }

                    let converted_samples = ret as usize;
                    out_samples.truncate(converted_samples * channels as usize);

                    // Convert timestamp to nanoseconds
                    let pts = (*frame).pts;
                    let timestamp = Self::ffmpeg_time_to_nanos(pts, timebase.0, timebase.1);

                    let audio_frame = AudioFrame {
                        data: out_samples,
                        sample_rate,
                        channels,
                        timestamp,
                    };

                    if tx.send(audio_frame).is_err() {
                        return Ok(());  // Receiver dropped
                    }
                }
            }

            Ok(())
        }
    }

    /// Convert FFmpeg timestamp to nanoseconds
    /// FFmpeg uses rational timebase: timestamp * (num/den) = seconds
    /// We convert to nanoseconds: timestamp * (num/den) * 1e9
    fn ffmpeg_time_to_nanos(pts: i64, num: i32, den: i32) -> Time {
        if den == 0 {
            return 0;
        }
        // Use i128 to avoid overflow
        let pts_i128 = pts as i128;
        let num_i128 = num as i128;
        let den_i128 = den as i128;
        let nanos_per_sec = 1_000_000_000i128;
        
        // Calculate: (pts * num * nanos_per_sec) / den
        let result = (pts_i128 * num_i128 * nanos_per_sec) / den_i128;
        result as i64
    }

    /// Convert nanoseconds to FFmpeg timestamp
    /// Reverse of ffmpeg_time_to_nanos
    fn nanos_to_ffmpeg_time(nanos: Time, num: i32, den: i32) -> i64 {
        if num == 0 {
            return 0;
        }
        // Use i128 to avoid overflow
        let nanos_i128 = nanos as i128;
        let num_i128 = num as i128;
        let den_i128 = den as i128;
        let nanos_per_sec = 1_000_000_000i128;
        
        // Calculate: (nanos * den) / (num * nanos_per_sec)
        let result = (nanos_i128 * den_i128) / (num_i128 * nanos_per_sec);
        result as i64
    }
}

impl Drop for FFmpegContext {
    fn drop(&mut self) {
        unsafe {
            // Free SwsContext
            if let Some(sws) = self.sws_ctx {
                ffmpeg_next::ffi::sws_freeContext(sws);
            }

            // Free SwrContext
            if let Some(swr) = self.swr_ctx {
                ffmpeg_next::ffi::swr_free(&mut swr);
            }

            // Free codec contexts
            if let Some(ctx) = self.video_codec_ctx {
                ffmpeg_next::ffi::avcodec_free_context(&mut ctx);
            }
            if let Some(ctx) = self.audio_codec_ctx {
                ffmpeg_next::ffi::avcodec_free_context(&mut ctx);
            }

            // Close format context
            if !self.format_ctx.is_null() {
                let mut format_ctx_ptr = self.format_ctx;
                ffmpeg_next::ffi::avformat_close_input(&mut format_ctx_ptr);
            }
        }
    }
}

// Notes on FFmpeg timebase conversion:
//
// FFmpeg uses rational timebases: timestamp * (num/den) = seconds
// Example: If timebase is (1, 1000), then timestamp 5000 = 5 seconds
//
// To convert FFmpeg timestamp to nanoseconds:
//   nanos = (timestamp * num * 1_000_000_000) / den
//
// To convert nanoseconds to FFmpeg timestamp:
//   timestamp = (nanos * den) / (num * 1_000_000_000)
//
// Common timebases:
// - (1, 1000) = milliseconds
// - (1, 1_000_000) = microseconds  
// - (1, 90_000) = MPEG-TS (common for H.264)
// - Variable per stream (check stream->time_base)
//
// Important: Always use the stream's time_base for accurate conversion.
// The format context may have a different time_base that's not accurate for individual streams.

