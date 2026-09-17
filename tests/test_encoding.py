import unittest
from pathlib import Path

class EncodingTests(unittest.TestCase):
    def test_source_and_documentation_have_no_replacement_runs(self):
        root = Path(__file__).resolve().parents[1]
        files = list(root.glob('*.md')) + list((root / 'docs').glob('*.md'))
        for folder in ['core', 'ui', 'system']:
            files.extend((root / folder).rglob('*.py'))
        files.append(root / 'main.py')
        for path in files:
            with self.subTest(path=path.name):
                text = path.read_text(encoding='utf-8')
                self.assertNotIn('?' * 3, text)
                self.assertNotIn(chr(0xfffd), text)
