// src/accounts.rs — extra Claude accounts shown next to the default sidebar tiles.
//
// Pure logic (no Qt): parsing and validating `accounts`, instance ids, the one label function,
// the tile list (`sidebar_tiles`, order + visibility) and the config writes of the menu. Mirrors
// `python/core/accounts.py`; the rules are in docs/PROVIDERS.md.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::Config;
use crate::providers::claude::{discover_profiles, resolve_active_profile};

pub const BASE_IDS: [&str; 3] = ["claude", "agy", "codex"];
pub const MAX_ACCOUNTS: usize = 6;
/// Characters of a label on a 64 px strip tile.
pub const TILE_LABEL_MAX: usize = 8;
/// Characters of a label in a panel card header (it shares the line with two badges).
pub const CARD_LABEL_MAX: usize = 13;
/// Characters of a label in a menu entry.
pub const MENU_LABEL_MAX: usize = 16;
const NICKNAME_MAX: usize = 32;
const PROFILE_MAX: usize = 128;

pub const KEY_ACCOUNTS: &str = "accounts";
pub const KEY_SIDEBAR_TILES: &str = "sidebar_tiles";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSpec {
    pub id: String,
    pub profile: String,
    pub nickname: String,
}

/// `claude@work` -> `claude`.
pub fn base_of(tile_id: &str) -> &str {
    tile_id.split('@').next().unwrap_or(tile_id)
}

/// `text` if it has at most `limit` chars, else its first `limit - 1` chars and `…`.
pub fn elide(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let head: String = text.chars().take(limit - 1).collect();
        format!("{head}\u{2026}")
    }
}

/// Case-folded profile id with ONE leading dot removed: the identity of a duplicate check.
pub fn profile_key(profile: &str) -> String {
    let text = profile.trim().to_lowercase();
    text.strip_prefix('.')
        .map_or_else(|| text.clone(), str::to_owned)
}

fn is_valid_id(id: &str) -> bool {
    let Some(slug) = id.strip_prefix("claude@") else {
        return false;
    };
    let mut chars = slug.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    first_ok
        && slug.chars().count() <= 16
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn profile_is_valid(profile: &str) -> bool {
    if profile.trim().is_empty() || profile.chars().count() > PROFILE_MAX {
        return false;
    }
    if profile.contains(['/', '\\', ':', '\0']) || matches!(profile.trim(), "." | "..") {
        return false;
    }
    !matches!(profile_key(profile).as_str(), "auto" | "default" | "claude")
}

/// The valid accounts of the `accounts` value and whether the whole value is usable: not a list ->
/// (empty, false); absent / null -> (empty, true). Invalid entries are skipped, never removed.
pub fn parse_accounts(raw: Option<&Value>) -> (Vec<AccountSpec>, bool) {
    let list = match raw {
        None | Some(Value::Null) => return (Vec::new(), true),
        Some(Value::Array(list)) => list,
        Some(_) => return (Vec::new(), false),
    };
    let mut specs: Vec<AccountSpec> = Vec::new();
    for entry in list {
        if specs.len() >= MAX_ACCOUNTS {
            break;
        }
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if entry.get("provider").and_then(Value::as_str) != Some("claude") {
            continue;
        }
        let (Some(id), Some(profile)) = (
            entry.get("id").and_then(Value::as_str),
            entry.get("profile").and_then(Value::as_str),
        ) else {
            continue;
        };
        if !is_valid_id(id) || !profile_is_valid(profile) {
            continue;
        }
        if specs
            .iter()
            .any(|s| s.id == id || profile_key(&s.profile) == profile_key(profile))
        {
            continue;
        }
        let nickname = entry
            .get("nickname")
            .and_then(Value::as_str)
            .map(|n| n.trim().chars().take(NICKNAME_MAX).collect::<String>())
            .unwrap_or_default();
        specs.push(AccountSpec {
            id: id.to_owned(),
            profile: profile.trim().to_owned(),
            nickname,
        });
    }
    (specs, true)
}

/// The valid accounts of a config (an unusable `accounts` value counts as absent).
pub fn accounts_of(cfg: &Config) -> Vec<AccountSpec> {
    parse_accounts(cfg.extra.get(KEY_ACCOUNTS)).0
}

fn account_slug(profile: &str) -> String {
    let mut slug = String::new();
    let mut gap = false;
    for c in profile_key(profile).chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !slug.is_empty() {
                slug.push('-');
            }
            gap = false;
            slug.push(c);
        } else {
            gap = true;
        }
    }
    let cut: String = slug.chars().take(16).collect();
    let cut = cut.trim_end_matches('-');
    if cut.is_empty() {
        "acct".to_owned()
    } else {
        cut.to_owned()
    }
}

/// `claude@<slug>`, `-2`, `-3` ... on a collision with `taken` (valid or not).
pub fn new_account_id(profile: &str, taken: &[String]) -> String {
    let slug = account_slug(profile);
    let mut candidate = format!("claude@{slug}");
    let mut n = 1;
    while taken.contains(&candidate) {
        n += 1;
        let suffix = format!("-{n}");
        let keep = 16 - suffix.len();
        let head: String = slug.chars().take(keep).collect();
        candidate = format!("claude@{}{suffix}", head.trim_end_matches('-'));
    }
    candidate
}

/// The tile ids of a config: every tile (`all`), the visible ones in order, the hidden ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tiles {
    pub all: Vec<String>,
    pub visible: Vec<String>,
    pub hidden: Vec<String>,
}

pub fn tiles_of(cfg: &Config, accounts: &[AccountSpec]) -> Tiles {
    let mut all: Vec<String> = Vec::new();
    for base in cfg.provider_order_setting() {
        all.push(base.clone());
        if base == "claude" {
            all.extend(accounts.iter().map(|a| a.id.clone()));
        }
    }
    let mut visible: Vec<String> = Vec::new();
    if let Some(Value::Array(list)) = cfg.extra.get(KEY_SIDEBAR_TILES) {
        for tile in list.iter().filter_map(Value::as_str) {
            if all.iter().any(|t| t == tile) && !visible.iter().any(|t| t == tile) {
                visible.push(tile.to_owned());
            }
        }
    }
    if visible.is_empty() {
        visible = all.clone();
    }
    let hidden = all
        .iter()
        .filter(|t| !visible.contains(t))
        .cloned()
        .collect();
    Tiles {
        all,
        visible,
        hidden,
    }
}

/// Whether the two config values the live apply reads are usable as a whole.
pub fn config_is_usable(cfg: &Config) -> bool {
    let tiles_ok = matches!(
        cfg.extra.get(KEY_SIDEBAR_TILES),
        None | Some(Value::Null | Value::Array(_))
    );
    parse_accounts(cfg.extra.get(KEY_ACCOUNTS)).1 && tiles_ok
}

fn leaf(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.strip_prefix('.')
        .map_or_else(|| name.clone(), str::to_owned)
}

fn norm(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase().replace('/', "\\")
    } else {
        text
    }
}

/// The one name of an account (tile, card, tray, tooltips): nickname, else the directory name
/// without its leading dot, widened with the parent only on a collision, else `account N`.
pub fn account_label(
    nickname: &str,
    profile_dir: &Path,
    other_dirs: &[PathBuf],
    ordinal: usize,
) -> String {
    let nickname = nickname.trim();
    if !nickname.is_empty() {
        return nickname.chars().take(NICKNAME_MAX).collect();
    }
    let own = leaf(profile_dir);
    if own.is_empty() {
        return format!("account {ordinal}");
    }
    let me = norm(profile_dir);
    for other in other_dirs {
        if norm(other) != me && leaf(other).to_lowercase() == own.to_lowercase() {
            let parent = profile_dir.parent().map(leaf).unwrap_or_default();
            return if parent.is_empty() {
                own
            } else {
                format!("{parent}/{own}")
            };
        }
    }
    own
}

/// An account with its directory and label, resolved against the profiles on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAccount {
    pub spec: AccountSpec,
    pub dir: PathBuf,
    pub label: String,
}

impl ResolvedAccount {
    /// The card header (and the provider's name): the label only, `elide(label, 13)`.
    pub fn provider_name(&self) -> String {
        elide(&self.label, CARD_LABEL_MAX)
    }

    /// The tile text and the tray header.
    pub fn tile_text(&self) -> String {
        elide(&self.label, TILE_LABEL_MAX)
    }
}

/// The valid accounts of `cfg`, each with its directory and label.
pub fn resolve_accounts(cfg: &Config) -> Vec<ResolvedAccount> {
    let specs = accounts_of(cfg);
    let (main, _) = resolve_active_profile(&cfg.claude_profile);
    let dirs: Vec<PathBuf> = specs
        .iter()
        .map(|s| resolve_active_profile(&s.profile).0.dir_path)
        .collect();
    specs
        .into_iter()
        .enumerate()
        .map(|(i, spec)| {
            let mut others = vec![main.dir_path.clone()];
            others.extend(
                dirs.iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, d)| d.clone()),
            );
            let label = account_label(&spec.nickname, &dirs[i], &others, i + 1);
            ResolvedAccount {
                spec,
                dir: dirs[i].clone(),
                label,
            }
        })
        .collect()
}

/// Whether `profile` is the one the main Claude tile is pinned to (never an extra account).
pub fn is_main_profile(cfg: &Config, profile: &str) -> bool {
    let pinned = cfg.claude_profile.trim();
    if pinned.is_empty() || pinned.eq_ignore_ascii_case("auto") {
        return false;
    }
    let (main, _) = resolve_active_profile(pinned);
    let (other, _) = resolve_active_profile(profile);
    main.id.eq_ignore_ascii_case(&other.id)
}

/// The profiles the menu offers: discovered, without `default` and without the pinned main one,
/// plus every account's own profile (so it can be unticked), then accounts whose profile is not
/// found. `(profile id, menu label, ticked, found)`.
pub fn menu_entries(cfg: &Config) -> Vec<(String, String, bool, bool)> {
    let accounts = accounts_of(cfg);
    let mut out: Vec<(String, String, bool, bool)> = Vec::new();
    for prof in discover_profiles() {
        let ticked = accounts
            .iter()
            .any(|a| profile_key(&a.profile) == profile_key(&prof.id));
        if prof.id == "default" || (!ticked && is_main_profile(cfg, &prof.id)) {
            continue;
        }
        out.push((prof.id.clone(), prof.display_name.clone(), ticked, true));
    }
    for account in &accounts {
        if !out
            .iter()
            .any(|(id, ..)| profile_key(id) == profile_key(&account.profile))
        {
            out.push((
                account.profile.clone(),
                format!("{}  找不到 (Not found)", account.profile),
                true,
                false,
            ));
        }
    }
    out
}

// ---- config writes (run inside `ConfigManager::update_and_save`) ------------------------------

fn raw_list(cfg: &Config, key: &str) -> Vec<Value> {
    match cfg.extra.get(key) {
        Some(Value::Array(list)) => list.clone(),
        _ => Vec::new(),
    }
}

/// Ticks `profile` on: appends the entry (raw entries, invalid ones included, are kept) and, when
/// `sidebar_tiles` is a list, its id. Refused (`None`) for the pinned main profile, a duplicate, the
/// cap, or an unusable `accounts` value.
pub fn add_account(cfg: &mut Config, profile: &str) -> Option<String> {
    if !profile_is_valid(profile) || !config_is_usable(cfg) {
        return None;
    }
    let (accounts, _) = parse_accounts(cfg.extra.get(KEY_ACCOUNTS));
    if accounts.len() >= MAX_ACCOUNTS
        || accounts
            .iter()
            .any(|a| profile_key(&a.profile) == profile_key(profile))
        || is_main_profile(cfg, profile)
    {
        return None;
    }
    let mut raw = raw_list(cfg, KEY_ACCOUNTS);
    let taken: Vec<String> = raw
        .iter()
        .filter_map(|e| e.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let id = new_account_id(profile, &taken);
    raw.push(json!({"id": id, "provider": "claude", "profile": profile.trim()}));
    cfg.extra.insert(KEY_ACCOUNTS.to_owned(), Value::Array(raw));
    if let Some(Value::Array(tiles)) = cfg.extra.get_mut(KEY_SIDEBAR_TILES) {
        tiles.push(Value::String(id.clone()));
    }
    Some(id)
}

/// Ticks an account off: its entry, its tile id and its `row_order` go. `tray_quota` stays.
pub fn remove_account(cfg: &mut Config, id: &str) {
    let mut raw = raw_list(cfg, KEY_ACCOUNTS);
    let before = raw.len();
    raw.retain(|e| e.get("id").and_then(Value::as_str) != Some(id));
    if raw.len() != before {
        cfg.extra.insert(KEY_ACCOUNTS.to_owned(), Value::Array(raw));
    }
    if let Some(Value::Array(tiles)) = cfg.extra.get_mut(KEY_SIDEBAR_TILES) {
        tiles.retain(|t| t.as_str() != Some(id));
    }
    cfg.row_order.remove(id);
}

/// Shows (appended at the end) or hides a tile. Hiding the only visible tile is refused.
pub fn set_tile_visible(cfg: &mut Config, id: &str, show: bool) -> bool {
    let tiles = tiles_of(cfg, &accounts_of(cfg));
    if !tiles.all.iter().any(|t| t == id) {
        return false;
    }
    let mut visible = tiles.visible;
    if show {
        if !visible.iter().any(|t| t == id) {
            visible.push(id.to_owned());
        }
    } else {
        if visible.len() == 1 {
            return false;
        }
        visible.retain(|t| t != id);
    }
    cfg.extra.insert(
        KEY_SIDEBAR_TILES.to_owned(),
        Value::Array(visible.into_iter().map(Value::String).collect()),
    );
    true
}

/// A drag reorder: `sidebar_tiles := order`, `provider_order := base ids in that order` followed
/// by the hidden base ids in their previous order (always a full permutation).
pub fn set_tile_order(cfg: &mut Config, order: &[String]) {
    let previous = cfg.provider_order_setting();
    let mut bases: Vec<String> = Vec::new();
    for id in order {
        let base = base_of(id).to_owned();
        if BASE_IDS.contains(&base.as_str()) && !bases.contains(&base) {
            bases.push(base);
        }
    }
    for base in previous {
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    cfg.provider_order = bases;
    cfg.extra.insert(
        KEY_SIDEBAR_TILES.to_owned(),
        Value::Array(order.iter().cloned().map(Value::String).collect()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn config_with(json: &str) -> Config {
        serde_json::from_str(json).expect("config")
    }

    #[test]
    fn label_rules() {
        assert_eq!(elide("claude-a", 8), "claude-a");
        assert_eq!(elide("claude-work", 8), "claude-\u{2026}");
        let dir = Path::new("/h/.claude-a");
        assert_eq!(account_label("", dir, &[], 1), "claude-a");
        assert_eq!(account_label("  Work  ", dir, &[], 1), "Work");
        assert_eq!(account_label("", Path::new("/"), &[], 3), "account 3");
        // the same leaf elsewhere widens with the parent
        let other = PathBuf::from("/x/claude-a");
        assert_eq!(account_label("", dir, &[other], 1), "h/claude-a");
    }

    #[test]
    fn card_header_is_the_label_and_the_tray_name_is_the_tile_text() {
        let account = ResolvedAccount {
            spec: AccountSpec {
                id: "claude@x".to_owned(),
                profile: "x".to_owned(),
                nickname: String::new(),
            },
            dir: PathBuf::from("/h/.x"),
            label: "a-very-long-account-label".to_owned(),
        };
        assert_eq!(account.provider_name(), "a-very-long-\u{2026}");
        let metrics = crate::providers::base::UsageMetrics {
            provider_id: "claude@x".to_owned(),
            provider_name: account.provider_name(),
            ..Default::default()
        };
        assert_eq!(
            crate::tray_quota::display_name_of(&metrics),
            account.tile_text()
        );
    }

    #[test]
    fn ids_and_validation() {
        assert_eq!(account_slug(".Claude A!"), "claude-a");
        assert_eq!(account_slug("..."), "acct");
        assert_eq!(new_account_id("claude-a", &[]), "claude@claude-a");
        assert_eq!(
            new_account_id("claude-a", &v(&["claude@claude-a"])),
            "claude@claude-a-2"
        );
        let raw = json!([
            {"id": "claude@a", "provider": "claude", "profile": "claude-a"},
            {"id": "claude@a", "provider": "claude", "profile": "other"},
            {"id": "claude@b", "provider": "claude", "profile": ".CLAUDE-A"},
            {"id": "claude@d", "provider": "claude", "profile": "default"},
            {"id": "Claude@e", "provider": "claude", "profile": "e"},
            {"id": "claude@f", "provider": "codex", "profile": "f"},
            "junk",
            {"id": "claude@g", "provider": "claude", "profile": "g", "nickname": " Gee "}
        ]);
        let (specs, ok) = parse_accounts(Some(&raw));
        assert!(ok);
        assert_eq!(
            specs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["claude@a", "claude@g"]
        );
        assert_eq!(specs[1].nickname, "Gee");
        assert!(!parse_accounts(Some(&json!("x"))).1);
    }

    #[test]
    fn old_config_gives_todays_three_tiles() {
        let cfg = Config::default();
        let t = tiles_of(&cfg, &accounts_of(&cfg));
        assert_eq!(t.visible, ["claude", "agy", "codex"]);
        assert!(t.hidden.is_empty());
    }

    #[test]
    fn tiles_follow_the_list_and_writes_keep_the_permutation() {
        let mut cfg = config_with(
            r#"{"accounts": [{"id": "claude@a", "provider": "claude", "profile": "a"}],
                "sidebar_tiles": ["codex", "claude@a", "nope", "codex"]}"#,
        );
        let t = tiles_of(&cfg, &accounts_of(&cfg));
        assert_eq!(t.all, ["claude", "claude@a", "agy", "codex"]);
        assert_eq!(t.visible, ["codex", "claude@a"]);
        assert_eq!(t.hidden, ["claude", "agy"]);
        set_tile_order(&mut cfg, &v(&["claude@a", "codex"]));
        assert_eq!(cfg.provider_order, ["claude", "codex", "agy"]);
        assert_eq!(cfg.extra[KEY_SIDEBAR_TILES], json!(["claude@a", "codex"]));
        // the last visible tile cannot be hidden
        assert!(set_tile_visible(&mut cfg, "claude@a", false));
        assert!(!set_tile_visible(&mut cfg, "codex", false));
        remove_account(&mut cfg, "claude@a");
        assert_eq!(cfg.extra[KEY_ACCOUNTS], json!([]));
    }
}
