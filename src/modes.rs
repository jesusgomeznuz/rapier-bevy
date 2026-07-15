//! La puerta del engine: preguntas puras sobre los flags del pipeline.
//! Nada muta, nada registra — se lee el comando y se responde.

pub fn record_duration() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--record")?;
    let secs = args
        .get(pos + 1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u32);
    Some(secs)
}

pub fn write_timeline_duration() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    reject_renamed_flag(&args, "--bake", "--write-timeline");
    reject_renamed_flag(&args, "--simulate", "--write-timeline");
    let pos = args.iter().position(|a| a == "--write-timeline")?;
    let secs = args
        .get(pos + 1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u32);
    Some(secs)
}

/// Cuánto dura la sesión de video — venga de --record o de --write-timeline.
/// La puerta (args.rs) de cada juego la consulta y la entrega como parámetro:
/// el juego recibe "la carrera dura N segundos" sin saber de flags.
pub fn session_duration_secs() -> Option<u32> {
    record_duration().or_else(write_timeline_duration)
}

pub fn timeline_path() -> Option<std::path::PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    reject_renamed_flag(&args, "--replay", "--play");
    let pos = args.iter().position(|a| a == "--play")?;
    let path = args
        .get(pos + 1)
        .expect("--play requiere la ruta de la timeline (ej. outputs/simulation_60s.timeline)");
    Some(std::path::PathBuf::from(path))
}

fn reject_renamed_flag(args: &[String], old: &str, new: &str) {
    if args.iter().any(|a| a == old) {
        eprintln!("{old} fue renombrado: usa {new}");
        std::process::exit(1);
    }
}
