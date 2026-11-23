"""Extra Claude accounts shown next to the default tiles. Pure logic, no Qt; see docs/PROVIDERS.md.

Config (both keys absent by default, `null` == absent):
  accounts       [{"id": "claude@claude-a", "provider": "claude", "profile": "claude-a", "nickname": "..."}]
  sidebar_tiles  ordered tile ids = order AND visibility of the sidebar tiles
Invalid entries are ignored at use and never rewritten. Ids, labels and row keys never contain a token, an
e-mail address or an account id: only the profile directory name.
"""
import os
import re
from dataclasses import dataclass
from typing import List, Optional, Tuple

BASE_IDS = ("claude", "agy", "codex")
MAX_ACCOUNTS = 6
TILE_LABEL_MAX = 8      # characters of a label on a 64 px strip tile
CARD_LABEL_MAX = 13     # characters in a panel card header (next to the two badges)
MENU_LABEL_MAX = 16     # characters in a menu entry
NICKNAME_MAX = 32
PROFILE_MAX = 128
ID_PATTERN = re.compile(r"^claude@[a-z0-9][a-z0-9_-]{0,15}\Z")


@dataclass(frozen=True)
class AccountSpec:
    id: str
    profile: str
    nickname: str = ""


def base_of(tile_id: str) -> str:
    return tile_id.split("@", 1)[0]


def elide(text: str, limit: int) -> str:
    return text if len(text) <= limit else text[:limit - 1] + "\u2026"


def profile_key(profile: str) -> str:
    """Case-folded profile id with ONE leading dot removed: the identity used for duplicate checks."""
    text = profile.strip().casefold()
    return text[1:] if text.startswith(".") else text


def profile_is_valid(profile) -> bool:
    if not isinstance(profile, str) or not profile.strip() or len(profile) > PROFILE_MAX:
        return False
    if any(c in profile for c in "/\\:\0") or profile.strip() in (".", ".."):
        return False
    return profile_key(profile) not in ("auto", "default", "claude")


def parse_accounts(raw) -> Tuple[List[AccountSpec], bool]:
    """(valid specs, whole value ok). Not a list -> ([], False); None/absent -> ([], True)."""
    if raw is None:
        return [], True
    if not isinstance(raw, list):
        return [], False
    specs, ids, profiles = [], set(), set()
    for entry in raw:
        if len(specs) >= MAX_ACCOUNTS:
            break
        if not isinstance(entry, dict) or entry.get("provider") != "claude":
            continue
        account_id, profile = entry.get("id"), entry.get("profile")
        if not isinstance(account_id, str) or not ID_PATTERN.match(account_id) or not profile_is_valid(profile):
            continue
        if account_id in ids or profile_key(profile) in profiles:
            continue
        nickname = entry.get("nickname")
        nickname = nickname.strip()[:NICKNAME_MAX] if isinstance(nickname, str) else ""
        ids.add(account_id)
        profiles.add(profile_key(profile))
        specs.append(AccountSpec(account_id, profile.strip(), nickname))
    return specs, True


def account_slug(profile: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", profile_key(profile)).strip("-")[:16].strip("-")
    return slug or "acct"


def new_account_id(profile: str, taken) -> str:
    taken = set(taken)
    slug = account_slug(profile)
    candidate, n = f"claude@{slug}", 1
    while candidate in taken:
        n += 1
        suffix = f"-{n}"
        candidate = f"claude@{slug[:16 - len(suffix)].rstrip('-')}{suffix}"
    return candidate


def valid_provider_order(raw) -> List[str]:
    if isinstance(raw, list) and all(isinstance(v, str) for v in raw) and sorted(raw) == sorted(BASE_IDS):
        return list(raw)
    return list(BASE_IDS)


def all_tile_ids(provider_order, accounts) -> List[str]:
    ids = []
    for base in provider_order:
        ids.append(base)
        if base == "claude":
            ids.extend(a.id for a in accounts)
    return ids


def resolve_tiles(provider_order, accounts, sidebar_tiles) -> Tuple[List[str], List[str], List[str]]:
    """(all ids, visible ids in order, hidden ids)."""
    everything = all_tile_ids(provider_order, accounts)
    visible = []
    if isinstance(sidebar_tiles, list):
        for tile in sidebar_tiles:
            if isinstance(tile, str) and tile in everything and tile not in visible:
                visible.append(tile)
    if not visible:
        visible = list(everything)
    return everything, visible, [t for t in everything if t not in visible]


def _leaf(path: str) -> str:
    leaf = os.path.basename(os.path.normpath(path)) if path else ""
    return leaf[1:] if leaf.startswith(".") else leaf


def account_label(nickname: Optional[str], profile_dir: str, other_dirs=(), ordinal: int = 1) -> str:
    """The one name used by the tile, card, tray and tooltips."""
    if isinstance(nickname, str) and nickname.strip():
        return nickname.strip()[:NICKNAME_MAX]
    leaf = _leaf(profile_dir)
    if not leaf:
        return f"account {ordinal}"
    me = os.path.normcase(os.path.normpath(profile_dir))
    for other in other_dirs:
        if os.path.normcase(os.path.normpath(other)) != me and _leaf(other).casefold() == leaf.casefold():
            parent = _leaf(os.path.dirname(os.path.normpath(profile_dir)))
            return f"{parent}/{leaf}" if parent else leaf
    return leaf
