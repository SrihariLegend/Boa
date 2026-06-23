use super::*;
pub(super) fn add_directory(tables: &mut Tablebase<Chess>, path: &Path) -> Result<usize, String> {
    let entries = std::fs::read_dir(path)
        .map_err(|err| format!("failed to read Syzygy path {}: {err}", path.display()))?;
    let mut files = 0usize;

    for entry in entries {
        let entry =
            entry.map_err(|err| format!("failed to read Syzygy path {}: {err}", path.display()))?;
        match tables.add_file(entry.path()) {
            Ok(()) => files += 1,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData
                ) =>
            {
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "failed to load Syzygy file {}: {err}",
                    entry.path().display()
                ));
            }
        }
    }

    Ok(files)
}

pub(super) fn split_syzygy_paths(path_list: &str) -> Vec<std::path::PathBuf> {
    if path_list.contains(';') {
        return path_list
            .split(';')
            .filter(|path| !path.trim().is_empty())
            .map(std::path::PathBuf::from)
            .collect();
    }
    std::env::split_paths(path_list).collect()
}

pub(super) fn can_probe(
    board: &Board,
    options: &SyzygyOptions,
    available_pieces: usize,
    depth: i32,
) -> bool {
    if options.probe_limit == 0 || depth < options.probe_depth as i32 {
        return false;
    }
    if board.castling != 0 {
        return false;
    }
    let pieces = bb_popcount(board.occ_all) as usize;
    pieces >= 2 && pieces <= options.probe_limit.min(available_pieces)
}
