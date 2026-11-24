//! Copy: an output put on the Windows clipboard as a file (`CF_HDROP`), the format Explorer puts
//! there for Ctrl+C on a file, so it pastes into an Explorer folder as a copy of the file and into
//! chat apps as an attachment.
//!
//! The payload is built by [`drop_files`], a pure function tested byte for byte, so no test needs
//! the real clipboard; [`copy_files`] only hands those bytes to Windows.

/// Size of the `DROPFILES` header that starts a `CF_HDROP` payload: `pFiles` (DWORD), `pt` (two
/// LONGs), `fNC` and `fWide` (BOOLs), each 4 bytes.
const DROPFILES_SIZE: u32 = 20;

/// `DROPEFFECT_COPY`, sent as the `Preferred DropEffect` format beside the file list so a paste
/// copies the output rather than moving it out of the destination.
const DROPEFFECT_COPY: u32 = 1;

/// The `CF_HDROP` payload naming `paths`: a `DROPFILES` header whose `pFiles` points just past
/// itself and whose `fWide` says the names are UTF-16, then each path in UTF-16LE with its NUL,
/// then one more NUL ending the list. Paths must be absolute; a drop target resolves nothing.
pub fn drop_files(paths: &[&str]) -> Vec<u8> {
    let mut payload = Vec::new();
    for field in [DROPFILES_SIZE, 0, 0, 0, 1] {
        // pFiles, pt.x, pt.y, fNC (the point is in client coordinates), fWide.
        payload.extend_from_slice(&field.to_le_bytes());
    }
    for path in paths {
        for unit in path.encode_utf16().chain([0]) {
            payload.extend_from_slice(&unit.to_le_bytes());
        }
    }
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload
}

/// Replaces the clipboard's contents with `paths` as files. Retries for about 50 ms while another
/// program holds the clipboard open. Fails with Windows' error text.
#[cfg(windows)]
pub fn copy_files(paths: &[&str]) -> Result<(), String> {
    use clipboard_win::{Clipboard, formats::CF_HDROP, raw};

    // Closes the clipboard when dropped, after both formats are set.
    let _open = Clipboard::new_attempts(10).map_err(|err| err.to_string())?;
    // `set` empties the clipboard first, taking it over from whatever was copied before.
    raw::set(CF_HDROP, &drop_files(paths)).map_err(|err| err.to_string())?;
    // Explorer copies on paste without it too; it only makes the intent explicit, so a failure
    // here leaves a working copy.
    if let Some(format) = raw::register_format("Preferred DropEffect") {
        let _ = raw::set_without_clear(format.get(), &DROPEFFECT_COPY.to_le_bytes());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn copy_files(_paths: &[&str]) -> Result<(), String> {
    Err("copying files to the clipboard is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dword(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// The UTF-16 units after the header.
    fn units(payload: &[u8]) -> Vec<u16> {
        payload[DROPFILES_SIZE as usize..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    }

    #[test]
    fn the_header_points_past_itself_and_marks_the_names_wide() {
        let payload = drop_files(&[r"C:\v\a.gif"]);
        assert_eq!(dword(&payload, 0), 20, "pFiles");
        assert_eq!(dword(&payload, 4), 0, "pt.x");
        assert_eq!(dword(&payload, 8), 0, "pt.y");
        assert_eq!(dword(&payload, 12), 0, "fNC");
        assert_eq!(dword(&payload, 16), 1, "fWide");
    }

    #[test]
    fn one_path_is_utf16le_ended_by_two_nuls() {
        let payload = drop_files(&[r"C:\a.gif"]);
        let mut expected = vec![20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
        for byte in r"C:\a.gif".bytes() {
            expected.extend([byte, 0]);
        }
        expected.extend([0, 0, 0, 0]);
        assert_eq!(payload, expected);
    }

    #[test]
    fn several_paths_are_each_nul_terminated_and_the_list_double_nul_terminated() {
        // Hangul is one UTF-16 unit per character; the emoji is a surrogate pair.
        let paths = [r"C:\v\언차일드.gif", r"D:\out\clip (1) 🎬.mp4"];
        let payload = drop_files(&paths);
        let units = units(&payload);
        assert_eq!(&units[units.len() - 2..], [0, 0]);
        let names: Vec<String> = units[..units.len() - 1]
            .split(|&unit| unit == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf16(name).unwrap())
            .collect();
        assert_eq!(names, paths);
        let expected_units: usize = paths.iter().map(|p| p.encode_utf16().count() + 1).sum();
        assert_eq!(payload.len(), 20 + 2 * (expected_units + 1));
    }
}
