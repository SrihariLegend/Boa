use super::*;
pub struct CriticalityLogger {
    log_dir: String,
    chunk_index: u64,
    max_chunk_bytes: u64,
    bytes_written: u64,
    writer: BufWriter<File>,
}

impl CriticalityLogger {
    pub fn open(log_dir: &str) -> io::Result<Option<Self>> {
        if let Ok(log_file) = std::env::var("BOA_CRITICALITY_LOG_FILE") {
            let log_file = log_file.trim();
            if !log_file.is_empty() {
                let path = Path::new(log_file);
                if let Some(parent) = path.parent() {
                    create_dir_all(parent)?;
                }
                let needs_header = metadata(path).map_or(true, |meta| meta.len() == 0);
                let file = OpenOptions::new().create(true).append(true).open(path)?;
                let mut writer = BufWriter::new(file);
                let mut bytes_written = metadata(path).map_or(0, |meta| meta.len());
                if needs_header {
                    writeln!(writer, "{}", CriticalityRecord::header())?;
                    bytes_written = CriticalityRecord::header().len() as u64 + 1;
                }
                return Ok(Some(CriticalityLogger {
                    log_dir: path
                        .parent()
                        .and_then(|parent| parent.to_str())
                        .unwrap_or("")
                        .to_string(),
                    chunk_index: 0,
                    max_chunk_bytes: 0,
                    bytes_written,
                    writer,
                }));
            }
        }

        if log_dir.trim().is_empty() {
            return Ok(None);
        }

        create_dir_all(log_dir)?;
        let max_chunk_bytes = std::env::var("BOA_CRITICALITY_MAX_CSV_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let mut chunk_index = latest_chunk_index(log_dir)?.unwrap_or(0);
        let mut path = chunk_path(log_dir, chunk_index);
        if max_chunk_bytes > 0 && metadata(&path).is_ok_and(|meta| meta.len() >= max_chunk_bytes) {
            chunk_index += 1;
            path = chunk_path(log_dir, chunk_index);
        }
        let needs_header = metadata(&path).map_or(true, |meta| meta.len() == 0);
        let mut bytes_written = metadata(&path).map_or(0, |meta| meta.len());
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut writer = BufWriter::new(file);
        if needs_header {
            writeln!(writer, "{}", CriticalityRecord::header())?;
            bytes_written = CriticalityRecord::header().len() as u64 + 1;
        }
        Ok(Some(CriticalityLogger {
            log_dir: log_dir.to_string(),
            chunk_index,
            max_chunk_bytes,
            bytes_written,
            writer,
        }))
    }

    pub fn write(&mut self, record: &CriticalityRecord) -> io::Result<()> {
        let row = record.to_csv_row();
        writeln!(self.writer, "{row}")?;
        self.bytes_written = self.bytes_written.saturating_add(row.len() as u64 + 1);
        if self.max_chunk_bytes > 0 && self.bytes_written >= self.max_chunk_bytes {
            self.rotate()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.writer.flush()?;
        self.chunk_index += 1;
        let path = chunk_path(&self.log_dir, self.chunk_index);
        let needs_header = metadata(&path).map_or(true, |meta| meta.len() == 0);
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        self.writer = BufWriter::new(file);
        self.bytes_written = 0;
        if needs_header {
            writeln!(self.writer, "{}", CriticalityRecord::header())?;
            self.bytes_written = CriticalityRecord::header().len() as u64 + 1;
        }
        Ok(())
    }
}

pub(super) fn chunk_path(log_dir: &str, chunk_index: u64) -> std::path::PathBuf {
    Path::new(log_dir).join(format!(
        "criticality-{}-{chunk_index:06}.csv",
        std::process::id()
    ))
}

pub(super) fn latest_chunk_index(log_dir: &str) -> io::Result<Option<u64>> {
    let prefix = format!("criticality-{}-", std::process::id());
    let mut latest = None;
    for entry in read_dir(log_dir)? {
        let entry = entry?;
        let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !file_name.starts_with(&prefix) || !file_name.ends_with(".csv") {
            continue;
        }
        let index = &file_name[prefix.len()..file_name.len() - 4];
        if let Ok(index) = index.parse::<u64>() {
            latest = Some(latest.map_or(index, |prev: u64| prev.max(index)));
        }
    }
    Ok(latest)
}
