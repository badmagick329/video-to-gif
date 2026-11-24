//! The startup check: are `ffmpeg` and `ffprobe` on PATH, and do they run?

use std::path::Path;

use crate::probe::domain::{FfmpegTools, Tool};
use crate::process;
use crate::search_path::SearchPath;

/// Locates both programs and asks each for its version, and checks that ffmpeg has the options
/// export needs. A found file that fails either check is reported unusable rather than found, so
/// the setup message also covers a broken or too old install.
pub fn find_tools(search: &SearchPath) -> FfmpegTools {
    let located = search.locate_tools();
    let check = |name: &str, found: Option<&Path>| match found {
        None => Tool::Missing,
        Some(path) => {
            let checked = read_version(path, name).and_then(|version| {
                if name == "ffmpeg" && !reports_frame_times(path) {
                    return Err(format!(
                        "ffmpeg 6.1 or newer is needed (this one is {version})"
                    ));
                }
                Ok(version)
            });
            match checked {
                Ok(version) => Tool::Found {
                    path: path.display().to_string(),
                    version,
                },
                Err(reason) => Tool::Unusable {
                    path: path.display().to_string(),
                    reason,
                },
            }
        }
    };
    FfmpegTools {
        ffmpeg: check("ffmpeg", located.ffmpeg.as_deref()),
        ffprobe: check("ffprobe", located.ffprobe.as_deref()),
    }
}

/// Runs `<program> -version` and returns the version token from its first line,
/// `"<name> version <version> Copyright ..."`. Any other answer means the file is not the program
/// its name claims.
fn read_version(program: &Path, name: &str) -> Result<String, String> {
    let output = process::command(program)
        .arg("-version")
        .output()
        .map_err(|err| format!("could not start: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("").trim();
        return Err(format!("`-version` failed ({}) {detail}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("");
    first_line
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(" version "))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .ok_or_else(|| format!("`-version` printed {first_line:?}, not an {name} version line"))
}

/// Whether `ffmpeg` writes per-frame encoder stats (`-stats_enc_pre`, ffmpeg 6.1 and later), which
/// export reads its progress from. Asked by capability rather than by version, since nightly
/// builds report `N-<commit>` instead of a release number: one 16×16 frame through the option,
/// about 40 ms, the same as `-version` (`-h full` takes twice that and prints 1.3 MB). An older
/// ffmpeg rejects the option and exits with an error.
fn reports_frame_times(ffmpeg: &Path) -> bool {
    process::command(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(["-f", "lavfi", "-i", "nullsrc=size=16x16", "-frames:v", "1"])
        .args([
            "-stats_enc_pre",
            "pipe:1",
            "-stats_enc_pre_fmt",
            "frame={n}",
        ])
        .args(["-f", "null", "-"])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("frame=0")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_tools_on_path_are_found_with_versions() {
        let tools = find_tools(&SearchPath::from_env());
        for (name, tool) in [("ffmpeg", &tools.ffmpeg), ("ffprobe", &tools.ffprobe)] {
            let Tool::Found { version, .. } = tool else {
                panic!("{name} must be on PATH for these tests: {tool:?}");
            };
            assert!(!version.is_empty(), "{name} version is empty");
        }
    }

    /// Guards against the resolver drifting from Windows' own search (`where.exe` prints matches
    /// in the order Windows tries them).
    #[cfg(windows)]
    #[test]
    fn ffmpeg_resolves_to_what_where_exe_reports_first() {
        let output = std::process::Command::new("where.exe")
            .arg("ffmpeg")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let expected = stdout.lines().next().expect("where.exe found no ffmpeg");

        let Tool::Found { path, .. } = find_tools(&SearchPath::from_env()).ffmpeg else {
            panic!("ffmpeg not found");
        };
        assert!(
            path.eq_ignore_ascii_case(expected.trim()),
            "{path} != {expected}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_file_that_is_not_a_program_is_unusable() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("ffmpeg.exe"), b"not a program").unwrap();
        let search = SearchPath::new(dir.path().as_os_str(), Some(".EXE".as_ref()));

        let tools = find_tools(&search);
        assert!(
            matches!(tools.ffmpeg, Tool::Unusable { .. }),
            "{:?}",
            tools.ffmpeg
        );
        assert_eq!(tools.ffprobe, Tool::Missing);
    }

    /// Answers `-version` as ffmpeg 6.0 does and rejects everything else the way ffmpeg rejects an
    /// option it lacks.
    #[cfg(windows)]
    const FFMPEG_6_0: &str = "@echo off
        if \"%1\"==\"-version\" (echo ffmpeg version 6.0 Copyright (c^) 2000-2023 the FFmpeg developers& exit /b 0)
        echo Unrecognized option 'stats_enc_pre'. 1>&2
        exit /b 8
";

    #[cfg(windows)]
    #[test]
    fn an_ffmpeg_without_per_frame_stats_is_unusable_as_too_old() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("ffmpeg.cmd"), FFMPEG_6_0).unwrap();
        let search = SearchPath::new(dir.path().as_os_str(), Some(".CMD".as_ref()));

        match find_tools(&search).ffmpeg {
            Tool::Unusable { reason, .. } => {
                assert_eq!(reason, "ffmpeg 6.1 or newer is needed (this one is 6.0)")
            }
            other => panic!("{other:?}"),
        }
    }
}
