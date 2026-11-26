use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CEPAGES: &[&str] = &[
    "merlot",
    "syrah",
    "pinot",
    "gamay",
    "chenin",
    "chardonnay",
    "sauvignon",
    "cabernet",
    "grenache",
    "mourvedre",
    "riesling",
    "viognier",
    "malbec",
    "meunier",
    "carignan",
    "cinsault",
    "gewurztraminer",
    "muscat",
    "semillon",
    "trousseau",
    "poulsard",
    "savagnin",
    "aligote",
    "marsanne",
    "roussanne",
    "mondeuse",
    "tannat",
    "colombard",
    "mauzac",
    "melon",
    "rolle",
    "ugni",
    "negrette",
    "fer-servadou",
    "petit-verdot",
    "picpoul",
    "bourboulenc",
    "clairette",
    "courbu",
    "gros-manseng",
    "petit-manseng",
    "jacquere",
    "altesse",
    "persan",
    "romorantin",
    "pineau",
    "menu-pineau",
];

static SEED_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Pick an unused grape variety name from `CEPAGES`.
/// When all base names are already present in `existing_short_names`, appends `-2`, `-3`, etc.
pub fn pick_unused_cepage(existing_short_names: &HashSet<String>) -> String {
    let available: Vec<&str> = CEPAGES
        .iter()
        .copied()
        .filter(|name| !existing_short_names.contains(*name))
        .collect();

    let seed = pseudo_random_index();

    if !available.is_empty() {
        return available[seed % available.len()].to_string();
    }

    for suffix in 2usize.. {
        for &base in CEPAGES {
            let candidate = format!("{}-{}", base, suffix);
            if !existing_short_names.contains(&candidate) {
                return candidate;
            }
        }
    }

    unreachable!()
}

fn pseudo_random_index() -> usize {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    let pid = std::process::id() as usize;
    let counter = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos
        .wrapping_add(pid.wrapping_mul(31))
        .wrapping_add(counter.wrapping_mul(17))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_unused_cepage_returns_valid_variety() {
        let existing = HashSet::new();
        let name = pick_unused_cepage(&existing);
        assert!(CEPAGES.contains(&name.as_str()));
    }

    #[test]
    fn test_pick_unused_cepage_avoids_existing_names() {
        let mut existing: HashSet<String> = CEPAGES.iter().skip(1).map(|s| s.to_string()).collect();
        let picked = pick_unused_cepage(&existing);
        assert_eq!(picked, CEPAGES[0]);

        existing.insert(picked);
        let overflow = pick_unused_cepage(&existing);
        assert_eq!(overflow, format!("{}-2", CEPAGES[0]));
    }
}
