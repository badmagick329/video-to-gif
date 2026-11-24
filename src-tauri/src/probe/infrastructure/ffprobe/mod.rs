//! Reading a source's metadata with ffprobe. ffprobe's JSON is untrusted input: it is parsed into
//! loose types here and validated into [`SourceInfo`] before anything else sees it.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::probe::domain::{ProbeError, SourceInfo, displayed_size};
use crate::process;

/// Highest frame rate accepted as real. Some demuxers report a timebase such as `90000/1` as
/// `r_frame_rate`; anything above this is treated as absent rather than trusted.
const MAX_FRAME_RATE: f64 = 1000.0;

/// Runs ffprobe on `file` and validates what it reports.
///
/// Options: `-show_streams -show_format` rather than `-show_entries`, because entry and section
/// names (e.g. `stream_side_data`) differ between ffprobe versions and older ones reject unknown
/// ones, while these two flags work everywhere (gif-studio 1.1.1 hit this). `-v error` keeps
/// stderr down to the reason a file failed.
pub fn probe(ffprobe: &Path, file: &Path) -> Result<SourceInfo, ProbeError> {
    let output = process::command(ffprobe)
        .args(["-v", "error", "-hide_banner"])
        .args(["-show_format", "-show_streams", "-of", "json"])
        .arg(file)
        .output()
        .map_err(|err| ProbeError::FfprobeFailed {
            reason: err.to_string(),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(|line| strip_path_prefix(line, file))
            .unwrap_or_else(|| format!("ffprobe exited with {}", output.status));
        return Err(ProbeError::Unreadable {
            path: file.display().to_string(),
            reason,
        });
    }
    parse(&output.stdout, file)
}

/// ffprobe prefixes errors with the input path (`C:\v.mp4: Invalid data ...`); the error already
/// carries the path, so keep only the message.
fn strip_path_prefix(line: &str, file: &Path) -> String {
    let prefix = format!("{}: ", file.display());
    line.strip_prefix(&prefix).unwrap_or(line).to_owned()
}

#[derive(Deserialize)]
struct RawOutput {
    #[serde(default)]
    streams: Vec<RawStream>,
    format: Option<RawFormat>,
}

#[derive(Deserialize)]
struct RawStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    side_data_list: Vec<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    tags: HashMap<String, serde_json::Value>,
    #[serde(default)]
    disposition: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

/// Validates ffprobe's `-of json` output for `file` into [`SourceInfo`].
fn parse(json: &[u8], file: &Path) -> Result<SourceInfo, ProbeError> {
    let path = file.display().to_string();
    let invalid = |reason: &str| ProbeError::InvalidOutput {
        path: path.clone(),
        reason: reason.to_owned(),
    };
    let raw: RawOutput = serde_json::from_slice(json)
        .map_err(|err| invalid(&format!("not the expected JSON: {err}")))?;

    // Cover art in MP3/M4A/MKV is a one-frame video stream flagged `attached_pic`; it is not the
    // picture the user means to turn into a GIF.
    let video = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video") && !is_attached_pic(s))
        .ok_or_else(|| ProbeError::NoVideoStream { path: path.clone() })?;
    // The first audio stream is the one the player and the proxy (`-map 0:a:0`) use.
    let audio_codec = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"))
        .map(|audio| non_empty(&audio.codec_name).unwrap_or_else(|| "unknown".to_owned()));

    let coded_width = positive_dimension(video.width).ok_or_else(|| invalid("no video width"))?;
    let coded_height =
        positive_dimension(video.height).ok_or_else(|| invalid("no video height"))?;
    let (width, height) = displayed_size(coded_width, coded_height, rotation(video));

    let frame_rate = [&video.avg_frame_rate, &video.r_frame_rate]
        .into_iter()
        .find_map(|rate| parse_rate(rate.as_deref()?))
        .ok_or_else(|| invalid("no usable frame rate"))?;

    let format = raw
        .format
        .as_ref()
        .ok_or_else(|| invalid("no format section"))?;
    // MKV and WebM leave per-stream duration out; the format's is the source's length anyway.
    let duration = [&format.duration, &video.duration]
        .into_iter()
        .find_map(|d| parse_seconds(d.as_deref()?))
        .ok_or_else(|| invalid("no duration"))?;

    let video_codec = non_empty(&video.codec_name).ok_or_else(|| invalid("no video codec"))?;
    let container = non_empty(&format.format_name).ok_or_else(|| invalid("no container"))?;

    Ok(SourceInfo {
        duration,
        width,
        height,
        frame_rate,
        video_codec,
        container,
        audio_codec,
    })
}

fn is_attached_pic(stream: &RawStream) -> bool {
    stream
        .disposition
        .get("attached_pic")
        .and_then(serde_json::Value::as_i64)
        == Some(1)
}

/// Rotation in degrees. Current ffprobe puts it in the display matrix side data; older builds and
/// some muxers only set the legacy `rotate` tag. Missing or unparseable means no rotation.
fn rotation(stream: &RawStream) -> f64 {
    let from_side_data = stream
        .side_data_list
        .iter()
        .find_map(|side_data| number(side_data.get("rotation")?));
    from_side_data
        .or_else(|| number(stream.tags.get("rotate")?))
        .unwrap_or(0.0)
}

/// A JSON number, or a string holding one (tags are always strings).
fn number(value: &serde_json::Value) -> Option<f64> {
    let n = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn positive_dimension(value: Option<i64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok()).filter(|&v| v > 0)
}

/// `"30000/1001"` or `"25"` as frames per second; `"0/0"` and implausible rates are `None`.
fn parse_rate(rate: &str) -> Option<f64> {
    let fps = match rate.split_once('/') {
        Some((num, den)) => num.trim().parse::<f64>().ok()? / den.trim().parse::<f64>().ok()?,
        None => rate.trim().parse().ok()?,
    };
    (fps.is_finite() && fps > 0.0 && fps <= MAX_FRAME_RATE).then_some(fps)
}

fn parse_seconds(value: &str) -> Option<f64> {
    let seconds: f64 = value.trim().parse().ok()?;
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
