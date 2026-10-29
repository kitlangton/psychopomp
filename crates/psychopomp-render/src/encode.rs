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
        for placement in media {
            let asset = placement.clip().asset();
            arguments.push("-i".to_owned());
            arguments.push(asset.path().to_string_lossy().into_owned());
        }
        if media.is_empty() {
            arguments.push("-an".to_owned());
        } else {
            arguments.extend([
                "-filter_complex".to_owned(),
                audio_filter_graph(media, 1),
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
                "-threads",
                "4",
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

pub(crate) fn audio_filter_graph(media: &[MediaPlacement], first_input: usize) -> String {
    let mut filters = media
        .iter()
        .enumerate()
        .map(|(index, placement)| audio_clip_filter(index, first_input + index, placement))
        .collect::<Vec<_>>();
    let inputs = (0..media.len())
        .map(|index| format!("[a{index}]"))
        .collect::<String>();
    filters.push(format!(
        "{inputs}amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.95:level=0:latency=1[aout]",
        media.len()
    ));
    filters.join(";")
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

    use super::{audio_clip_filter, temporary_output_path};

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
