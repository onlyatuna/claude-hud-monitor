use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::text::font::{Font, SharedFontData};
use crate::text::glyph_face::{parse_face, SharedGlyphFace};
use crate::text::glyph_layout::FontEngine;

use rustybuzz::ttf_parser::name::Table as NameTable;
use rustybuzz::ttf_parser::{name_id, PlatformId};

/// Family metadata discovered from a font face's `name` and `post` tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamilyInfo {
    /// Family name (typographic family when present, legacy family otherwise).
    pub family: String,
    /// File the face was found in; `None` for application fonts registered from memory.
    pub path: Option<PathBuf>,
    /// Face index inside a font collection (`.ttc`/`.otc`); 0 for single-face files.
    pub face_index: u32,
    /// `post.isFixedPitch`: every glyph has the same advance (monospaced).
    pub fixed_pitch: bool,
}

/// System font cache database (`QFontDatabase` equivalent).
///
/// Searches, loads, and caches system fonts to avoid redundant disk I/O and parsing.
pub struct FontDatabase {
    cache: HashMap<String, SharedGlyphFace>,
    raw_cache: HashMap<String, SharedFontData>,
    file_cache: HashMap<PathBuf, SharedFontData>,
    search_paths: Vec<PathBuf>,
    /// Faces found by scanning `search_paths`; built lazily by [`FontDatabase::families`].
    system_faces: Option<Vec<FontFamilyInfo>>,
    /// Faces registered through [`FontDatabase::add_font_from_memory`].
    application_faces: Vec<FontFamilyInfo>,
    /// One engine per requested family string, shared so glyph caches persist across draw calls.
    engines: HashMap<String, FontEngine>,
}

/// Bumped whenever the set of available fonts changes, so caches derived from font data
/// (e.g. text advance widths) know to drop their entries.
static FONT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Current font-set generation; changes whenever a search path or an in-memory font is added.
pub fn font_generation() -> u64 {
    FONT_GENERATION.load(Ordering::Relaxed)
}

/// Process-wide font database shared by the painter and font-selection widgets.
static GLOBAL_FONT_DATABASE: Mutex<Option<FontDatabase>> = Mutex::new(None);

/// Runs `f` with the process-wide font database, creating it on first use.
///
/// Mirrors Qt's static `QFontDatabase` API: fonts registered here are visible to
/// text rendering and to widgets such as `FontComboBox`.
pub fn with_global_font_database<R>(f: impl FnOnce(&mut FontDatabase) -> R) -> R {
    let mut guard = GLOBAL_FONT_DATABASE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(FontDatabase::new))
}
/// Resolves the primary font engine and its platform-aware fallback chain (`QFontEngineMulti` equivalent)
/// using the process-wide global font database.
pub fn resolve_font_engines_global(font: &Font) -> Vec<FontEngine> {
    with_global_font_database(|db| db.resolve_font_engines(font))
}

/// Like [`resolve_font_engines_global`], but loads the fallback fonts only when `text` needs them.
///
/// Parsing a CJK/emoji fallback costs 200-500 ms each the first time with the `fontdue` backend,
/// and the fallbacks are only ever consulted for characters the primary font has no glyph for
/// (`GlyphLayout::partition_into_runs`). Text the primary font fully covers is therefore shaped
/// with the primary engine alone; anything else gets the same full chain as before.
pub fn resolve_font_engines_for_text_global(font: &Font, text: &str) -> Vec<FontEngine> {
    with_global_font_database(|db| db.resolve_font_engines_for_text(font, text))
}

impl FontDatabase {
    /// Creates a new font database and preloads default fonts.
    pub fn new() -> Self {
        let _t = crate::startup_trace::span(|| "FontDatabase::new".into());
        let mut search_paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            if let Ok(windir) = std::env::var("WINDIR") {
                search_paths.push(PathBuf::from(windir).join("Fonts"));
            } else {
                search_paths.push(PathBuf::from("C:\\Windows\\Fonts"));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            search_paths.push(PathBuf::from("/usr/share/fonts"));
            search_paths.push(PathBuf::from("/System/Library/Fonts"));
        }

        let mut db = Self {
            cache: HashMap::new(),
            raw_cache: HashMap::new(),
            file_cache: HashMap::new(),
            search_paths,
            system_faces: None,
            application_faces: Vec::new(),
            engines: HashMap::new(),
        };

        db.preload_default_fonts();
        db
    }

    /// Adds a directory to the font search paths.
    pub fn add_search_path<P: AsRef<Path>>(&mut self, path: P) {
        self.search_paths.push(path.as_ref().to_path_buf());
        self.system_faces = None;
        FONT_GENERATION.fetch_add(1, Ordering::Relaxed);
    }

    /// Registers a custom font from memory.
    pub fn add_font_from_memory(
        &mut self,
        family: &str,
        data: impl Into<SharedFontData>,
    ) -> Result<SharedGlyphFace, String> {
        let shared = data.into();
        let font_arc = parse_face(shared.as_slice(), 0)
            .map_err(|e| format!("Failed to parse font from memory: {e}"))?;
        let key = family.to_ascii_lowercase();
        let fixed_pitch = read_faces(&mut std::io::Cursor::new(shared.as_slice()))
            .first()
            .map(|face| face.fixed_pitch)
            .unwrap_or(false);
        self.application_faces
            .retain(|face| !face.family.eq_ignore_ascii_case(family));
        self.application_faces.push(FontFamilyInfo {
            family: family.to_string(),
            path: None,
            face_index: 0,
            fixed_pitch,
        });
        self.cache.insert(key.clone(), Arc::clone(&font_arc));
        self.raw_cache.insert(key, shared);
        FONT_GENERATION.fetch_add(1, Ordering::Relaxed);
        Ok(font_arc)
    }

    /// Sorted, de-duplicated family names of every installed and application font
    /// (`QFontDatabase::families`).
    pub fn families(&mut self) -> Vec<String> {
        self.collect_families(|_| true)
    }

    /// Families whose faces are all fixed-pitch (`QFontDatabase::isFixedPitch` filter).
    pub fn monospaced_families(&mut self) -> Vec<String> {
        let fixed: Vec<String> = self.collect_families(|face| face.fixed_pitch);
        fixed
            .into_iter()
            .filter(|family| self.is_fixed_pitch(family))
            .collect()
    }

    /// Families with at least one proportional (non fixed-pitch) face.
    pub fn proportional_families(&mut self) -> Vec<String> {
        self.collect_families(|face| !face.fixed_pitch)
    }

    /// Returns `true` if a family with this name (case-insensitive) is installed or registered.
    pub fn has_family(&mut self, family: &str) -> bool {
        self.faces()
            .any(|face| face.family.eq_ignore_ascii_case(family))
    }

    /// Returns `true` if every known face of `family` is fixed-pitch (`QFontDatabase::isFixedPitch`).
    pub fn is_fixed_pitch(&mut self, family: &str) -> bool {
        let mut matched = false;
        for face in self.faces() {
            if face.family.eq_ignore_ascii_case(family) {
                if !face.fixed_pitch {
                    return false;
                }
                matched = true;
            }
        }
        matched
    }

    /// Every face known to the database (system scan plus application fonts).
    fn faces(&mut self) -> impl Iterator<Item = &FontFamilyInfo> {
        if self.system_faces.is_none() {
            self.system_faces = Some(scan_font_directories(&self.search_paths));
        }
        self.system_faces
            .iter()
            .flatten()
            .chain(self.application_faces.iter())
    }

    fn collect_families(&mut self, mut keep: impl FnMut(&FontFamilyInfo) -> bool) -> Vec<String> {
        let mut names: Vec<String> = self
            .faces()
            .filter(|face| keep(face))
            .map(|face| face.family.clone())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    /// Loads a font by family name, caching the result.
    pub fn load_font(&mut self, family: &str) -> Option<SharedGlyphFace> {
        if family.contains(',') {
            for candidate in family.split(',') {
                let clean = candidate.trim().trim_matches('\'').trim_matches('"');
                if clean.is_empty()
                    || clean.eq_ignore_ascii_case("sans-serif")
                    || clean.eq_ignore_ascii_case("monospace")
                    || clean.eq_ignore_ascii_case("serif")
                {
                    continue;
                }
                if let Some(f) = self.load_font(clean) {
                    return Some(f);
                }
            }
        }

        let key = family.to_ascii_lowercase();
        if let Some(font) = self.cache.get(&key) {
            return Some(Arc::clone(font));
        }

        if let Some((raw_data, face)) = self.find_and_load_font_file(&key) {
            self.cache.insert(key.clone(), Arc::clone(&face));
            self.raw_cache.insert(key, raw_data);
            return Some(face);
        }

        if key != "segoe ui" && key != "arial" {
            if let Some(fallback) = self
                .load_font("segoe ui")
                .or_else(|| self.load_font("arial"))
            {
                return Some(fallback);
            }
        }

        None
    }

    /// Returns the raw binary font data for OpenType shaping.
    pub fn get_raw_font_data(&mut self, family: &str) -> Option<SharedFontData> {
        let key = family.to_ascii_lowercase();
        if let Some(data) = self.raw_cache.get(&key) {
            return Some(data.clone());
        }
        self.load_font(family);
        self.raw_cache.get(&key).cloned()
    }
    /// Resolves the primary font engine and its platform-aware fallback chain (`QFontEngineMulti` equivalent).
    ///
    /// The resulting engines are ordered:
    /// 1. Primary requested font (or in-memory custom font if provided in `font.font_data`)
    /// 2. CJK / Multilingual script fallback fonts
    /// 3. Symbol and Emoji fallback fonts
    pub fn resolve_font_engines(&mut self, font: &Font) -> Vec<FontEngine> {
        let _t = crate::startup_trace::span_min(0.5, || format!("resolve_font_engines({:?})", font.family));
        let mut engines = self.resolve_primary_engine(font);
        self.append_fallback_engines(&mut engines, None);
        engines
    }

    /// [`resolve_font_engines`](Self::resolve_font_engines), but fallback fonts are loaded only
    /// while `text` still has a character no engine resolved so far has a glyph for.
    ///
    /// The result is a prefix of the full chain, and `GlyphLayout::partition_into_runs` only
    /// consults a fallback for characters the earlier engines lack, so shaping is unchanged.
    pub fn resolve_font_engines_for_text(&mut self, font: &Font, text: &str) -> Vec<FontEngine> {
        let _t = crate::startup_trace::span_min(0.5, || format!("resolve_font_engines_for_text({:?})", font.family));
        let mut engines = self.resolve_primary_engine(font);
        self.append_fallback_engines(&mut engines, Some(text));
        engines
    }

    /// The font requested by `font` alone (empty when the family cannot be loaded).
    fn resolve_primary_engine(&mut self, font: &Font) -> Vec<FontEngine> {
        let mut engines: Vec<FontEngine> = Vec::with_capacity(4);

        // 1. Primary in-memory font data if specified
        if let Some(shared) = &font.font_data {
            if let Ok(face) = parse_face(shared.as_slice(), 0) {
                engines.push(
                    FontEngine::new(face)
                        .with_raw_data(shared.clone())
                        .with_face_index(0),
                );
            }
        }

        // If no in-memory font or in-memory font failed to parse, load primary by family
        if engines.is_empty() {
            self.try_add_engine(&mut engines, &font.family);
        }
        engines
    }

    /// Appends the platform's fallback fonts in priority order: CJK first, then emoji/symbols.
    /// With `needed_for`, stops as soon as every character of that text has a glyph in `engines`.
    fn append_fallback_engines(&mut self, engines: &mut Vec<FontEngine>, needed_for: Option<&str>) {
        #[cfg(target_os = "windows")]
        const SLOTS: &[&[&str]] = &[
            &["Microsoft JhengHei", "Microsoft YaHei"],
            &["Segoe UI Emoji", "Segoe UI Symbol"],
        ];
        #[cfg(target_os = "macos")]
        const SLOTS: &[&[&str]] = &[
            &["PingFang TC", "PingFang SC", "Heiti TC"],
            &["Apple Color Emoji", "Apple Symbols"],
        ];
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        const SLOTS: &[&[&str]] = &[
            &["Noto Sans CJK TC", "Noto Sans CJK SC", "WenQuanYi Micro Hei"],
            &["Noto Color Emoji", "DejaVu Sans"],
        ];

        for slot in SLOTS {
            if let Some(text) = needed_for {
                let covered = !engines.is_empty()
                    && text.chars().all(|ch| {
                        matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{200b}')
                            || engines.iter().any(|e| e.face.glyph_index(ch) != 0)
                    });
                if covered {
                    break;
                }
            }
            for family in *slot {
                if self.try_add_engine(engines, family) {
                    break;
                }
            }
        }
    }

    fn try_add_engine(&mut self, engines: &mut Vec<FontEngine>, fam: &str) -> bool {
        if let Some(font_face) = self.load_font(fam) {
            if engines.iter().any(|e| Arc::ptr_eq(&e.face, &font_face)) {
                return false;
            }
            // Reuse the engine built for this exact request so its glyph caches survive across calls
            // (Qt keeps one QFontEngine per font and caches glyphs in it). Keyed by the requested
            // family string, not by font: a CSS-style list such as "'Segoe UI', sans-serif" has no
            // raw data while the plain name "Segoe UI" does, and the two shape differently
            // (face metrics vs rustybuzz). A replaced font has a new Arc, so a stale entry is
            // never matched.
            if let Some(engine) = self.engines.get(fam).filter(|e| Arc::ptr_eq(&e.face, &font_face)) {
                engines.push(engine.clone());
                return true;
            }
            let raw_data = self.get_raw_font_data(fam);
            let mut engine = FontEngine::new(font_face).with_face_index(0);
            if let Some(raw) = raw_data {
                engine = engine.with_raw_data(raw);
            }
            self.engines.insert(fam.to_string(), engine.clone());
            engines.push(engine);
            true
        } else {
            false
        }
    }

    /// Returns the raw binary font data by canonical or relative file path.
    pub fn get_raw_font_data_by_path(&mut self, path: &Path) -> Option<SharedFontData> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(data) = self.file_cache.get(&canonical) {
            return Some(data.clone());
        }
        if let Ok(bytes) = std::fs::read(&canonical) {
            let shared = SharedFontData::from_vec(bytes);
            self.file_cache.insert(canonical, shared.clone());
            return Some(shared);
        }
        None
    }

    /// Returns the total bytes across all cached binary font files.
    pub fn total_raw_bytes(&self) -> usize {
        self.file_cache.values().map(|d| d.len()).sum()
    }

    /// Returns the number of distinct font files cached in memory.
    pub fn cached_files_count(&self) -> usize {
        self.file_cache.len()
    }

    /// Purges cached font data that is no longer referenced outside the database cache.
    ///
    /// Mirrors Qt's `QFontCache::decreaseCache`: inspects strong reference counts and
    /// purges unused raw buffers from memory.
    pub fn cleanup_unused_fonts(&mut self) -> usize {
        let mut freed_bytes = 0;
        self.file_cache.retain(|_path, data| {
            if data.strong_count() <= 2 {
                freed_bytes += data.len();
                false
            } else {
                true
            }
        });
        self.raw_cache.retain(|_fam, data| data.strong_count() > 1);
        self.cache.retain(|_fam, font| Arc::strong_count(font) > 1);
        freed_bytes
    }

    /// Preloads common default system fonts.
    fn preload_default_fonts(&mut self) {
        let default_families = ["segoe ui", "arial", "consolas"];
        for fam in &default_families {
            if let Some((raw_data, face)) = self.find_and_load_font_file(fam) {
                self.cache.insert((*fam).to_string(), face);
                self.raw_cache.insert((*fam).to_string(), raw_data);
                break;
            }
        }
    }

    /// Searches search paths for a font file matching family key, reusing existing
    /// cached binary buffers from `file_cache` to eliminate redundant disk I/O and memory duplication.
    fn find_and_load_font_file(&mut self, family_key: &str) -> Option<(SharedFontData, SharedGlyphFace)> {
        let _t = crate::startup_trace::span(|| format!("find_and_load_font_file({family_key:?})"));
        let candidate_filenames: Vec<String> = match family_key {
            "segoe ui" => vec!["segoeui.ttf".into(), "SegoeUI.ttf".into()],
            "segoe ui symbol" => vec!["seguisym.ttf".into()],
            "segoe ui emoji" => vec!["seguiemj.ttf".into()],
            "arial" => vec!["arial.ttf".into(), "Arial.ttf".into()],
            "consolas" => vec![
                "consola.ttf".into(),
                "Consola.ttf".into(),
                "consolas.ttf".into(),
            ],
            "tahoma" => vec!["tahoma.ttf".into(), "Tahoma.ttf".into()],
            "microsoft jhenghei" | "msjh" => vec![
                "msjh.ttc".into(),
                "msjh.ttf".into(),
                "msjhl.ttc".into(),
                "msjhbd.ttc".into(),
            ],
            "microsoft yahei" | "msyh" => vec![
                "msyh.ttc".into(),
                "msyh.ttf".into(),
                "msyhl.ttc".into(),
                "msyhbd.ttc".into(),
            ],
            "ms gothic" => vec!["msgothic.ttc".into(), "msgothic.ttf".into()],
            other => {
                let no_space = other.replace(' ', "");
                vec![
                    format!("{}.ttf", other),
                    format!("{}.otf", other),
                    format!("{}.ttf", no_space),
                    format!("{}.otf", no_space),
                ]
            }
        }
        .into_iter()
        .map(|s| s.to_string())
        .collect();

        for dir in &self.search_paths {
            for filename in &candidate_filenames {
                let file_path = dir.join(filename);
                if file_path.is_file() {
                    let canonical = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());
                    if let Some(shared) = self.file_cache.get(&canonical) {
                        if let Ok(face) = parse_face(shared.as_slice(), 0) {
                            return Some((shared.clone(), face));
                        }
                    }
                    let read = crate::startup_trace::span(|| format!("fs::read {}", canonical.display()));
                    if let Ok(bytes) = std::fs::read(&canonical) {
                        drop(read);
                        let _p = crate::startup_trace::span(|| format!("parse_face {} ({} KB)", canonical.display(), bytes.len() / 1024));
                        let shared = SharedFontData::from_vec(bytes);
                        if let Ok(face) = parse_face(shared.as_slice(), 0) {
                            self.file_cache.insert(canonical, shared.clone());
                            return Some((shared, face));
                        }
                    }
                }
            }
        }

        // Fall back to the family index (file names rarely match family names, e.g. "Times New Roman" -> times.ttf).
        if self.system_faces.is_none() {
            let _t = crate::startup_trace::span(|| format!("scan_font_directories (needed by {family_key:?})"));
            self.system_faces = Some(scan_font_directories(&self.search_paths));
        }
        let (face_path, face_index) = {
            let face = self
                .system_faces
                .as_ref()?
                .iter()
                .find(|face| face.family.eq_ignore_ascii_case(family_key) && face.path.is_some())?;
            (face.path.as_ref()?.clone(), face.face_index)
        };

        let canonical = face_path.canonicalize().unwrap_or_else(|_| face_path.clone());

        if let Some(shared) = self.file_cache.get(&canonical) {
            let face = parse_face(shared.as_slice(), face_index).ok()?;
            return Some((shared.clone(), face));
        }

        let _t = crate::startup_trace::span(|| format!("read+parse (family index) {}", canonical.display()));
        let bytes = std::fs::read(&canonical).ok()?;
        let shared = SharedFontData::from_vec(bytes);
        let face = parse_face(shared.as_slice(), face_index).ok()?;
        self.file_cache.insert(canonical, shared.clone());
        Some((shared, face))
    }
}

/// Maximum directory depth scanned below each search path (Linux font trees are nested).
const MAX_SCAN_DEPTH: usize = 4;

/// Scans font directories for `.ttf`/`.otf`/`.ttc`/`.otc` files and reads their family names.
fn scan_font_directories(roots: &[PathBuf]) -> Vec<FontFamilyInfo> {
    let mut faces = Vec::new();
    let mut pending: Vec<(PathBuf, usize)> = roots.iter().map(|root| (root.clone(), 0)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < MAX_SCAN_DEPTH {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            let is_font = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "ttf" | "otf" | "ttc" | "otc"
                    )
                })
                .unwrap_or(false);
            if !is_font {
                continue;
            }
            let Ok(mut file) = File::open(&path) else {
                continue;
            };
            for mut face in read_faces(&mut file) {
                face.path = Some(path.clone());
                faces.push(face);
            }
        }
    }
    faces
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_exact_at<R: Read + Seek>(reader: &mut R, offset: u64, len: usize) -> Option<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Reads family metadata for every face of an sfnt file or collection, touching only
/// the header, table directory, `name` and `post` tables.
fn read_faces<R: Read + Seek>(reader: &mut R) -> Vec<FontFamilyInfo> {
    let Some(header) = read_exact_at(reader, 0, 12) else {
        return Vec::new();
    };
    let face_offsets: Vec<u32> = if &header[0..4] == b"ttcf" {
        let count = be_u32(&header, 8).unwrap_or(0).min(256) as usize;
        match read_exact_at(reader, 12, count * 4) {
            Some(table) => (0..count).filter_map(|i| be_u32(&table, i * 4)).collect(),
            None => return Vec::new(),
        }
    } else {
        vec![0]
    };
    face_offsets
        .into_iter()
        .enumerate()
        .filter_map(|(index, offset)| read_face(reader, offset as u64, index as u32))
        .collect()
}

/// Maximum `name` table size accepted (real tables are a few KiB).
const MAX_NAME_TABLE_LEN: u32 = 1 << 20;

fn read_face<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    face_index: u32,
) -> Option<FontFamilyInfo> {
    let offset_table = read_exact_at(reader, offset, 12)?;
    let num_tables = be_u16(&offset_table, 4)? as usize;
    let records = read_exact_at(reader, offset + 12, num_tables * 16)?;
    let mut name_table = None;
    let mut post_table = None;
    for i in 0..num_tables {
        let record = &records[i * 16..i * 16 + 16];
        let table_offset = be_u32(record, 8)?;
        let table_len = be_u32(record, 12)?;
        match &record[0..4] {
            b"name" => name_table = Some((table_offset, table_len)),
            b"post" => post_table = Some((table_offset, table_len)),
            _ => {}
        }
    }
    let (name_offset, name_len) = name_table?;
    if name_len > MAX_NAME_TABLE_LEN {
        return None;
    }
    let name_data = read_exact_at(reader, name_offset as u64, name_len as usize)?;
    let family = preferred_family_name(&NameTable::parse(&name_data)?)?;
    // post table: version (4), italicAngle (4), underlinePosition (2), underlineThickness (2), isFixedPitch (4).
    let fixed_pitch = post_table
        .filter(|&(_, len)| len >= 16)
        .and_then(|(post_offset, _)| read_exact_at(reader, post_offset as u64 + 12, 4))
        .and_then(|bytes| be_u32(&bytes, 0))
        .map(|flag| flag != 0)
        .unwrap_or(false);
    Some(FontFamilyInfo {
        family,
        path: None,
        face_index,
        fixed_pitch,
    })
}

/// Picks the family name, preferring the typographic family (name ID 16) over the legacy
/// family (ID 1) and US-English Windows records over other languages.
fn preferred_family_name(table: &NameTable) -> Option<String> {
    const WINDOWS_EN_US: u16 = 0x0409;
    let mut best: Option<(u32, String)> = None;
    for name in table.names {
        let id_rank = match name.name_id {
            name_id::TYPOGRAPHIC_FAMILY => 0,
            name_id::FAMILY => 1,
            _ => continue,
        };
        let lang_rank = match (name.platform_id, name.language_id) {
            (PlatformId::Windows, WINDOWS_EN_US) => 0,
            (PlatformId::Unicode, _) => 1,
            (PlatformId::Macintosh, 0) => 2,
            _ => 3,
        };
        let rank = id_rank * 4 + lang_rank;
        if best
            .as_ref()
            .is_some_and(|(best_rank, _)| *best_rank <= rank)
        {
            continue;
        }
        let text = if name.is_unicode() {
            name.to_string()
        } else if name.platform_id == PlatformId::Macintosh && name.encoding_id == 0 {
            // Mac Roman: ASCII range is identical; drop anything outside it.
            Some(
                name.name
                    .iter()
                    .filter(|b| b.is_ascii())
                    .map(|&b| b as char)
                    .collect(),
            )
        } else {
            None
        };
        if let Some(text) = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, text)| text)
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_database_creation_and_lookup() {
        let mut db = FontDatabase::new();
        let font = db.load_font("Arial").or_else(|| db.load_font("Segoe UI"));
        assert!(
            font.is_some(),
            "Windows system fonts should include Arial or Segoe UI"
        );

        let cached = db.load_font("Arial").or_else(|| db.load_font("Segoe UI"));
        assert!(cached.is_some());
        assert!(Arc::ptr_eq(&font.unwrap(), &cached.unwrap()));
    }

    #[test]
    fn test_font_database_memory_registration() {
        let mut db = FontDatabase::new();
        let path = Path::new("C:/Windows/Fonts/arial.ttf");
        if path.exists() {
            let data = Arc::new(std::fs::read(path).unwrap());
            let res = db.add_font_from_memory("CustomArial", data.clone());
            assert!(res.is_ok());

            let loaded = db.load_font("CustomArial");
            assert!(loaded.is_some());

            let raw = db.get_raw_font_data("CustomArial");
            assert!(raw.is_some());
            assert_eq!(raw.unwrap().len(), data.len());
        }
    }
}
