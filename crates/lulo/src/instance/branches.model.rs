//! Project Zomboid's Steam branches (the game versions an instance can pin), fetched live.
//!
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub use steam::pics::Branch;

const PROJECT_ZOMBOID: u32 = 108600;

/// Opening an instance asks again, so the cache only spares repeated fetches within minutes.
const TTL: Duration = Duration::from_secs(600);
static CACHE: Mutex<Option<(Instant, Vec<Branch>)>> = Mutex::new(None);

/// Branches that can be installed (password-protected ones are left out).
pub async fn fetch() -> Result<Vec<Branch>, String> {
    if let Some((at, cached)) = CACHE.lock().expect("cache lock").clone()
        && at.elapsed() < TTL
    {
        return Ok(cached);
    }
    let branches = crate::runtime::spawn(async {
        let http = steam::http();
        let mut client = steam::Client::anonymous(&http).await?;
        let info = steam::pics::app_info(&mut client, &http, PROJECT_ZOMBOID).await?;
        Ok::<_, steam::Error>(steam::pics::branches(&info))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())?;
    let usable: Vec<_> = branches
        .into_iter()
        .filter(|b| !b.password_required)
        .collect();
    *CACHE.lock().expect("cache lock") = Some((Instant::now(), usable.clone()));
    Ok(usable)
}

/// "Nov 6, 2026" for a Unix time, in UTC.
pub fn date(unix: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // Days to civil date (Howard Hinnant's algorithm).
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{} {day}, {year}", MONTHS[(month - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::date;

    #[test]
    fn formats_branch_dates() {
        assert_eq!(date(0), "Jan 1, 1970");
        assert_eq!(date(1_793_974_760), "Nov 6, 2026");
        assert_eq!(date(1_787_663_102), "Aug 25, 2026");
        assert_eq!(date(951_782_400), "Feb 29, 2000");
    }
}
