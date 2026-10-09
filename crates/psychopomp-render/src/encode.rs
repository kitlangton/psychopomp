use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, bail};

use psychopomp::composition::MediaPlacement;

static NEXT_TEMPORARY_OUTPUT: AtomicU64 = AtomicU64::new(0);

/// Converts RGB frames to 4:2:0 with the BT.709 matrix and tags every frame as
/// BT.709 limited range, so the H.264 stream declares the matrix, primaries,
/// and transfer players must decode with. Untagged output is converted with
/// BT.601 while players assume BT.709 for HD, which shifts saturated colors.
/// The tags ride on the frames because FFmpeg lets frame properties override
/// `-color_primaries` and `-color_trc`, which would otherwise stay unspecified.
const BT709_FILTER: &str = "scale=out_color_matrix=bt709:out_range=tv,\
setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv";

#[derive(Clone, Copy)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

pub struct FfmpegEncoder {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    frame_bytes: usize,
    output: PathBuf,
    temporary_output: PathBuf,
    finished: bool,
}

impl FfmpegEncoder {
    pub fn start_with_media(
        output: &Path,
        spec: VideoSpec,
        media: &[MediaPlacement],
    ) -> Result<Self> {
        let file_name = output
            .file_name()
            .context("output path must include a file name")?
            .to_string_lossy();
        let temporary_output = temporary_output_path(output, &file_name);
        let temporary_output_string = temporary_output.to_string_lossy().into_owned();
        let mut arguments = [
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{}x{}", spec.width, spec.height),
            "-framerate",
            &spec.fps.to_string(),
            "-i",
            "-",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let mut filters = Vec::with_capacity(media.len());
        for (index, placement) in media.iter().enumerate() {
            let asset = placement.clip().asset();
            arguments.push("-i".to_owned());
            arguments.push(asset.path().to_string_lossy().into_owned());
            filters.push(audio_clip_filter(index, index + 1, placement));
        }
        if media.is_empty() {
            arguments.push("-an".to_owned());
        } else {
            let inputs = (0..media.len())
                .map(|index| format!("[a{index}]"))
                .collect::<String>();
            filters.push(format!(
                "{inputs}amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.95:level=0:latency=1[aout]",
                media.len()
            ));
            arguments.extend([
                "-filter_complex".to_owned(),
                filters.join(";"),
                "-map".to_owned(),
                "0:v:0".to_owned(),
                "-map".to_owned(),
                "[aout]".to_owned(),
                "-c:a".to_owned(),
                "aac".to_owned(),
                "-b:a".to_owned(),
                "192k".to_owned(),
            ]);
        }
        arguments.extend(
            [
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "17",
                // Film grain and bloom are expensive noise to encode; cap the rate so
                // a grainy minute stays shareable while clean frames keep CRF quality.
                "-maxrate",
                "14M",
                "-bufsize",
                "28M",
                "-vf",
                BT709_FILTER,
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                &temporary_output_string,
            ]
            .into_iter()
            .map(str::to_owned),
        );
        let mut child = Command::new("ffmpeg")
            .args(&arguments)
            .stdin(Stdio::piped())
            .spawn()
            .context("start FFmpeg")?;
        let stdin = child.stdin.take().context("open FFmpeg stdin")?;

        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            frame_bytes: spec.width as usize * spec.height as usize * 4,
            output: output.to_owned(),
            temporary_output,
            finished: false,
        })
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<()> {
        if rgba.len() != self.frame_bytes {
            bail!(
                "expected {} RGBA frame bytes, received {}",
                self.frame_bytes,
                rgba.len()
            );
        }
        self.stdin
            .as_mut()
            .context("FFmpeg input is already closed")?
            .write_all(rgba)
            .context("stream frame to FFmpeg")
    }

    pub fn finish(mut self) -> Result<()> {
        self.stdin.take();
        let status = self
            .child
            .as_mut()
            .context("FFmpeg process is already closed")?
            .wait()
            .context("wait for FFmpeg")?;
        self.child.take();
        if !status.success() {
            bail!("FFmpeg exited with {status}");
        }
        fs::rename(&self.temporary_output, &self.output).with_context(|| {
            format!(
                "move completed video from {} to {}",
                self.temporary_output.display(),
                self.output.display()
            )
        })?;
        self.finished = true;
        Ok(())
    }
}

fn temporary_output_path(output: &Path, file_name: &str) -> PathBuf {
    let nonce = NEXT_TEMPORARY_OUTPUT.fetch_add(1, Ordering::Relaxed);
    output.with_file_name(format!(
        ".{file_name}.psychopomp-tmp-{}-{nonce}.mp4",
        std::process::id()
    ))
}

fn audio_clip_filter(
    output_index: usize,
    input_index: usize,
    placement: &MediaPlacement,
) -> String {
    let source = placement.clip().source_range();
    let delay_ms = placement.timeline_range().start().as_nanos() as f64 / 1_000_000.0;
    format!(
        "[{input_index}:a]atrim=start={}:end={},volume={:.3}dB,asetpts=PTS-STARTPTS,adelay={delay_ms:.6}:all=1[a{output_index}]",
        source.start(),
        source.end(),
        placement.clip().audio_gain_db(),
    )
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        self.stdin.take();
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if !self.finished {
            let _ = fs::remove_file(&self.temporary_output);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use psychopomp::composition::{Asset, MediaPlacement, MediaRole, Time, TimeRange};

    use super::{FfmpegEncoder, VideoSpec, audio_clip_filter, temporary_output_path};

    #[test]
    #[ignore = "requires FFmpeg; encodes a flat frame and decodes it with the tagged matrix"]
    fn encoded_video_is_tagged_bt709_and_decodes_the_source_color() {
        use std::process::Command;

        const SOURCE: [u8; 3] = [0x41, 0x8C, 0xFF];
        const SIZE: u32 = 64;
        let output =
            std::env::temp_dir().join(format!("psychopomp-bt709-{}.mp4", std::process::id()));
        let spec = VideoSpec {
            width: SIZE,
            height: SIZE,
            fps: 30,
        };
        let frame = [SOURCE[0], SOURCE[1], SOURCE[2], 255].repeat((SIZE * SIZE) as usize);
        let mut encoder = FfmpegEncoder::start_with_media(&output, spec, &[]).unwrap();
        for _ in 0..3 {
            encoder.write_frame(&frame).unwrap();
        }
        encoder.finish().unwrap();

        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
            .arg("stream=color_space,color_primaries,color_transfer,color_range")
            .args(["-of", "default=noprint_wrappers=1"])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&probe.stdout),
            "color_range=tv\ncolor_space=bt709\ncolor_transfer=bt709\ncolor_primaries=bt709\n"
        );

        let yuv = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "yuv420p",
                "-",
            ])
            .output()
            .unwrap()
            .stdout;
        let _ = std::fs::remove_file(&output);
        let area = (SIZE * SIZE) as usize;
        let center = (SIZE / 2) as usize;
        let luma = yuv[center * SIZE as usize + center];
        let chroma = (center / 2) * (SIZE as usize / 2) + center / 2;
        let (cb, cr) = (yuv[area + chroma], yuv[area + area / 4 + chroma]);
        // BT.709 limited range back to RGB.
        let y = (f32::from(luma) - 16.0) / 219.0;
        let cb = (f32::from(cb) - 128.0) / 224.0;
        let cr = (f32::from(cr) - 128.0) / 224.0;
        let r = y + 1.5748 * cr;
        let b = y + 1.8556 * cb;
        let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
        let decoded = [r, g, b].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
        for (decoded, source) in decoded.into_iter().zip(SOURCE) {
            assert!(
                decoded.abs_diff(source) <= 1,
                "decoded {decoded:?} from {SOURCE:?}"
            );
        }
    }

    #[test]
    fn audio_placement_uses_delay_instead_of_positive_pts_offsets() {
        let clip = Asset::audio("cue", "cue.wav")
            .clip(TimeRange::new(Time::seconds(0.25), Time::seconds(0.75)))
            .gain_db(12.0);
        let placement = MediaPlacement::new(clip, MediaRole::Layer, Time::seconds(1.2345));
        let filter = audio_clip_filter(2, 3, &placement);

        assert_eq!(
            filter,
            "[3:a]atrim=start=0.250000000:end=0.750000000,volume=12.000dB,asetpts=PTS-STARTPTS,adelay=1234.500000:all=1[a2]"
        );
    }

    #[test]
    fn concurrent_encoders_use_distinct_temporary_outputs() {
        let output = Path::new("output/lesson.mp4");

        assert_ne!(
            temporary_output_path(output, "lesson.mp4"),
            temporary_output_path(output, "lesson.mp4")
        );
    }
}
