/// Progression d'une opération réseau git (`--progress`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub phase: String,
    pub percent: Option<u8>,
    pub current: u64,
    pub total: Option<u64>,
}

/// Interprète une ligne de progression de git, par exemple
/// `Receiving objects:  45% (123/273), 1.20 MiB | 2.00 MiB/s` ou
/// `remote: Enumerating objects: 15, done.`
pub fn parse_progress(line: &str) -> Option<Progress> {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map(str::trim_start).unwrap_or(line);
    let (phase, rest) = line.split_once(':')?;
    let phase = phase.trim();
    if phase.is_empty() || phase.contains("http") {
        return None;
    }
    let rest = rest.trim_start();
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }

    let mut percent = None;
    let mut current = None;
    let mut total = None;
    if let Some(pct_pos) = rest.find('%') {
        percent = rest[..pct_pos].trim().parse::<u8>().ok();
        let after = &rest[pct_pos + 1..];
        if let (Some(open), Some(close)) = (after.find('('), after.find(')'))
            && let Some((a, b)) = after[open + 1..close].split_once('/')
        {
            current = a.trim().parse().ok();
            total = b.trim().parse().ok();
        }
    } else {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        current = digits.parse().ok();
    }

    Some(Progress { phase: phase.to_string(), percent, current: current.or(percent.map(u64::from))?, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_parses_receiving() {
        let p = parse_progress("Receiving objects:  45% (123/273), 1.2 MiB | 2 MiB/s").unwrap();
        assert_eq!(p.phase, "Receiving objects");
        assert_eq!(p.percent, Some(45));
        assert_eq!(p.current, 123);
        assert_eq!(p.total, Some(273));
    }

    #[test]
    fn progress_parses_remote_count_without_percent() {
        let p = parse_progress("remote: Enumerating objects: 15, done.").unwrap();
        assert_eq!(p.phase, "Enumerating objects");
        assert_eq!(p.percent, None);
        assert_eq!(p.current, 15);
        assert_eq!(p.total, None);
    }

    #[test]
    fn progress_parses_done_line() {
        let p = parse_progress("Resolving deltas: 100% (50/50), done.").unwrap();
        assert_eq!(p.percent, Some(100));
        assert_eq!(p.total, Some(50));
    }

    #[test]
    fn progress_ignores_plain_messages() {
        assert!(parse_progress("Cloning into 'repo'...").is_none());
        assert!(parse_progress("From https://example.com/repo").is_none());
        assert!(parse_progress("").is_none());
    }
}
