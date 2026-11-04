import ast
import re
import tempfile
import unittest
from pathlib import Path

from PyInstaller.building.icon import normalize_icon_type


class PackagingTests(unittest.TestCase):
    def test_macos_png_icon_can_be_converted_to_icns(self):
        icon = Path(__file__).resolve().parents[1] / 'assets' / 'app_icon.png'
        with tempfile.TemporaryDirectory() as directory:
            converted = Path(normalize_icon_type(str(icon), ('icns',), 'icns', directory))
            self.assertEqual(converted.suffix, '.icns')
            self.assertEqual(converted.read_bytes()[:4], b'icns')
            self.assertGreater(converted.stat().st_size, 8)


    def test_spec_does_not_exclude_a_qt_module_the_app_imports(self):
        """The frozen exe crashed on start once because the spec excluded QtNetwork, which single_instance.py needs."""
        root = Path(__file__).resolve().parents[1]
        values = {}
        for node in ast.parse((root / 'ClaudeHUD.spec').read_text(encoding='utf-8')).body:
            if isinstance(node, ast.Assign) and getattr(node.targets[0], 'id', '') in ('EXCLUDES', 'EXCLUDED_MODULE_SUBSTRINGS'):
                values[node.targets[0].id] = ast.literal_eval(node.value)
        excluded = set(values['EXCLUDES'])
        substrings = values['EXCLUDED_MODULE_SUBSTRINGS']
        pattern = re.compile(r'^\s*(?:from\s+(PySide6(?:\.\w+)?)\s+import|import\s+(PySide6\.\w+))', re.M)
        used = {}
        for folder in ('core', 'system', 'ui'):
            for path in (root / folder).rglob('*.py'):
                for match in pattern.finditer(path.read_text(encoding='utf-8')):
                    module = match.group(1) or match.group(2)
                    if module != 'PySide6':
                        used.setdefault(module, path.name)
        main = root / 'main.py'
        for match in pattern.finditer(main.read_text(encoding='utf-8')):
            module = match.group(1) or match.group(2)
            if module != 'PySide6':
                used.setdefault(module, main.name)
        self.assertIn('PySide6.QtNetwork', used)  # guards the scan itself
        for module, where in used.items():
            self.assertNotIn(module, excluded, f'{where} imports {module} but ClaudeHUD.spec excludes it')
            name = module.rsplit('.', 1)[-1].lower()
            for part in substrings:
                self.assertNotIn(part, (name, 'qt6' + name[2:]), f'{where} imports {module} but the spec filters "{part}" out')
