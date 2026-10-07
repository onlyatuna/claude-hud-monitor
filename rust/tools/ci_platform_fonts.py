#!/usr/bin/env python3
"""CI diagnostic: what the platform's own font database knows vs. what qtrs scans.

Qt does not scan directories. It asks the platform (qtbase/src/gui/text):
  * Linux : fontconfig  (unix/qfontconfigdatabase.cpp: FcFontList / FcFontSort / FcConfigSubstitute)
  * macOS : CoreText    (coretext/qcoretextfontdatabase.mm: font manager + cascade lists)
qtrs scans /usr/share/fonts and /System/Library/Fonts only. This script prints, for every
family that `font_probe` reported as NOT INSTALLED, whether the platform database has it, and
which directories hold fonts that qtrs never looks at. It prints only; it never fails the build.

usage: ci_platform_fonts.py <font-probe-output.txt>
"""
import collections
import json
import os
import platform
import re
import subprocess
import sys

QTRS_ROOTS = ("/usr/share/fonts", "/System/Library/Fonts")


def missing_families(probe_path):
    names = []
    with open(probe_path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = re.match(r"NOT INSTALLED: (.+?) \(", line)
            if m and m.group(1) not in names:
                names.append(m.group(1))
    return names


def scanned_by_qtrs(path):
    return any(path == r or path.startswith(r + "/") for r in QTRS_ROOTS)


def dir_histogram(paths):
    counts = collections.Counter(os.path.dirname(p) for p in paths)
    print("directories holding fonts (qtrs scans: *):")
    for d, n in sorted(counts.items()):
        print("%s %s: %d" % ("*" if scanned_by_qtrs(d) else " ", d, n))
    outside = sum(n for d, n in counts.items() if not scanned_by_qtrs(d))
    print("font files outside the directories qtrs scans: %d of %d" % (outside, sum(counts.values())))


def macos(missing):
    out = subprocess.run(
        ["system_profiler", "SPFontsDataType", "-json"],
        capture_output=True, text=True, timeout=600,
    ).stdout
    items = json.loads(out).get("SPFontsDataType", [])
    families = collections.defaultdict(set)
    paths = set()
    for it in items:
        p = it.get("path", "")
        paths.add(p)
        for tf in it.get("typefaces", []):
            fam = tf.get("family")
            if fam:
                families[fam.lower()].add(p)
    print("CoreText (system_profiler) knows %d families in %d files" % (len(families), len(paths)))
    dir_histogram(paths)
    print("--- requested families the CoreText database has ---")
    for name in missing:
        hit = sorted(families.get(name.lower(), ()))
        if hit:
            tag = "" if scanned_by_qtrs(os.path.dirname(hit[0])) else "  (outside qtrs scan dirs)"
            print("CoreText HAS    %-24s %s%s" % (name, hit[0], tag))
        else:
            print("CoreText LACKS  %-24s" % name)


def linux(missing):
    out = subprocess.run(
        ["fc-list", "--format", "%{file}\t%{family}\n"],
        capture_output=True, text=True,
    ).stdout
    paths, families = set(), set()
    for line in out.splitlines():
        p, _, fam = line.partition("\t")
        paths.add(p)
        families.update(x.strip().lower() for x in fam.split(","))
    print("fontconfig knows %d families in %d files" % (len(families), len(paths)))
    dir_histogram(paths)
    print("--- requested families: fontconfig substitution (what Qt would use) ---")
    for name in missing:
        m = subprocess.run(["fc-match", name, "family", "file"], capture_output=True, text=True).stdout.strip()
        print("%-24s -> %s%s" % (name, m, "" if name.lower() not in families else "  [installed]"))


def main():
    probe = sys.argv[1] if len(sys.argv) > 1 else ""
    missing = missing_families(probe) if probe and os.path.exists(probe) else []
    system = platform.system()
    print("platform:", system, platform.release())
    print("requested families not installed (per font_probe): %s" % (", ".join(missing) or "(none)"))
    try:
        if system == "Darwin":
            macos(missing)
        elif system == "Linux":
            linux(missing)
        else:
            print("(Windows: qtrs scans %WINDIR%\\Fonts like Qt's GDI/FT database; nothing to compare)")
    except Exception as e:  # diagnostics only
        print("diagnostic failed: %r" % (e,))
    return 0


if __name__ == "__main__":
    sys.exit(main())
