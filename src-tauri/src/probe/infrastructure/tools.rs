//! The startup check: are `ffmpeg` and `ffprobe` on PATH, and do they run?

use std::path::Path;

use crate::probe::domain::{FfmpegTools, Tool};
use crate::probe::infrastructure::process;
use crate::probe::infrastructure::search_path::SearchPath;

/// Locates both programs and asks each for its version. A found file that fails `-version` is
/// reported unusable rather than found, so the setup message also covers a broken install.
pub fn find_tools(search: &SearchPath) -> FfmpegTools {
    let located = search.locate_tools();
    let check = |name: &str, found: Option<&Path>| match found {
        None => Tool::Missing,
        Some(path) => match read_version(path, name) {
            Ok(version) => Tool::Found {
                path: path.display().to_string(),
                version,
            },
            Err(reason) => Tool::Unusable {
                path: path.display().to_string(),
                reason,
            },
        },
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
}
