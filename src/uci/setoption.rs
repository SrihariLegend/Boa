use super::*;
pub(super) fn handle_setoption<'a>(
    tokens: impl Iterator<Item = &'a str>,
    tt: &mut TranspositionTable,
    contempt: &mut i32,
    options: &mut EngineOptions,
    syzygy: &mut Option<SyzygyTablebase>,
) {
    let mut name_parts = Vec::new();
    let mut value_parts = Vec::new();
    let mut reading_name = false;
    let mut reading_value = false;
    for tok in tokens {
        match tok {
            "name" => {
                reading_name = true;
                reading_value = false;
            }
            "value" => {
                reading_value = true;
                reading_name = false;
            }
            t => {
                if reading_name {
                    name_parts.push(t);
                }
                if reading_value {
                    value_parts.push(t);
                }
            }
        }
    }
    let name = name_parts.join(" ");
    let val = value_parts.join(" ");
    let name_key = name.to_ascii_lowercase().replace(' ', "");
    match name_key.as_str() {
        "hash" => {
            let mb: usize = val.parse().unwrap_or(128);
            *tt = TranspositionTable::new(mb.clamp(1, 4096));
        }
        "contempt" => {
            *contempt = val.parse().unwrap_or(0);
        }
        "syzygypath" => {
            options.syzygy.path = val.clone();
            match SyzygyTablebase::load(&val) {
                Ok(next) => {
                    if let Some(tb) = next.as_ref() {
                        println!(
                            "info string loaded {} Syzygy files, max pieces {}",
                            tb.file_count(),
                            tb.max_pieces()
                        );
                    } else {
                        println!("info string Syzygy disabled");
                    }
                    *syzygy = next;
                }
                Err(err) => {
                    eprintln!("info string SyzygyPath error: {err}");
                    *syzygy = None;
                }
            }
        }
        _ => {
            let _ = options.set_uci_option(&name, &val);
        }
    }
}
