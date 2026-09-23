import base64
import json
import os
import tempfile
import unittest
from unittest.mock import patch, MagicMock

from core.providers.agy_provider import (
    AgyProfile,
    extract_jwt_email,
    discover_profiles,
    resolve_active_profile,
    switch_active_profile,
    save_current_as_profile,
    set_keyring_credential_raw,
    get_keyring_credential_raw,
    AgyProvider,
)


class AgyProfileTests(unittest.TestCase):
    def test_extract_jwt_email_valid(self):
        payload = {"email": "user@example.com", "name": "Test User"}
        payload_b64 = base64.urlsafe_b64encode(json.dumps(payload).encode("utf-8")).decode("ascii").rstrip("=")
        fake_jwt = f"header.{payload_b64}.signature"
        email, name = extract_jwt_email(fake_jwt)
        self.assertEqual(email, "user@example.com")
        self.assertEqual(name, "Test User")

    def test_extract_jwt_email_urlsafe_and_padding(self):
        payload = {"email": "foo-bar_baz@example.com", "name": "Special ~!@#$ Name"}
        payload_b64 = base64.urlsafe_b64encode(json.dumps(payload).encode("utf-8")).decode("ascii").rstrip("=")
        fake_jwt = f"eyJhbGciOiJSUzI1NiJ9.{payload_b64}.c2lnbmF0dXJl"
        email, name = extract_jwt_email(fake_jwt)
        self.assertEqual(email, "foo-bar_baz@example.com")
        self.assertEqual(name, "Special ~!@#$ Name")

    def test_extract_jwt_email_invalid(self):
        self.assertEqual(extract_jwt_email(None), (None, None))
        self.assertEqual(extract_jwt_email(""), (None, None))
        self.assertEqual(extract_jwt_email("invalid"), (None, None))
        self.assertEqual(extract_jwt_email("part1.part2"), (None, None))
        self.assertEqual(extract_jwt_email("header.%%%invalidb64%%%.sig"), (None, None))

    @patch("core.providers.agy_provider.get_keyring_credential_raw")
    def test_auto_capture_and_discovery(self, mock_get_raw):
        with tempfile.TemporaryDirectory() as tmp_dir:
            profiles_dir = os.path.join(tmp_dir, "profiles")
            with patch("core.providers.agy_provider.get_profiles_dir", return_value=profiles_dir):
                # 1. Setup mock keyring credential
                payload = {"email": "evan@google.com", "name": "Evan"}
                payload_b64 = base64.urlsafe_b64encode(json.dumps(payload).encode("utf-8")).decode("ascii").rstrip("=")
                fake_jwt = f"header.{payload_b64}.sig"
                mock_cred = {
                    "token": {"access_token": "ya29.test12345", "refresh_token": "1//test"},
                    "id_token": fake_jwt
                }
                mock_get_raw.return_value = mock_cred

                # 2. Add another saved profile to profiles_dir
                os.makedirs(profiles_dir, exist_ok=True)
                work_path = os.path.join(profiles_dir, "work.json")
                with open(work_path, "w", encoding="utf-8") as f:
                    json.dump({
                        "id": "work",
                        "email": "work@company.com",
                        "short_name": "work",
                        "display_name": "work (work@company.com)",
                        "credential": {
                            "token": {"access_token": "ya29.worktoken"}
                        }
                    }, f)

                # 3. Discover profiles
                profs = discover_profiles()
                self.assertGreaterEqual(len(profs), 2)
                # Active profile should be first
                self.assertTrue(profs[0].is_keyring_active)
                self.assertEqual(profs[0].email, "evan@google.com")
                self.assertEqual(profs[0].short_name, "evan")

                # The other profile should also be present
                work_prof = next((p for p in profs if p.id == "work"), None)
                self.assertIsNotNone(work_prof)
                self.assertEqual(work_prof.email, "work@company.com")

    @patch("core.providers.agy_provider.discover_profiles")
    def test_resolve_active_profile(self, mock_discover):
        prof_active = AgyProfile(id="personal", display_name="personal (p@g.com)", short_name="personal", email="p@g.com", is_keyring_active=True)
        prof_work = AgyProfile(id="work", display_name="work (w@c.com)", short_name="work", email="w@c.com", is_keyring_active=False)
        mock_discover.return_value = [prof_active, prof_work]

        # Auto resolution
        resolved, is_auto = resolve_active_profile("auto")
        self.assertTrue(is_auto)
        self.assertEqual(resolved.id, "personal")

        # Specific resolution by id
        resolved_work, is_auto_work = resolve_active_profile("work")
        self.assertFalse(is_auto_work)
        self.assertEqual(resolved_work.id, "work")

        # Specific resolution by email
        resolved_email, is_auto_email = resolve_active_profile("w@c.com")
        self.assertFalse(is_auto_email)
        self.assertEqual(resolved_email.id, "work")

        # Unknown resolution fallback
        resolved_unknown, is_auto_unk = resolve_active_profile("custom_prof")
        self.assertFalse(is_auto_unk)
        self.assertEqual(resolved_unknown.id, "custom_prof")

    @patch("core.providers.agy_provider.set_keyring_credential_raw")
    @patch("core.providers.agy_provider.discover_profiles")
    def test_switch_active_profile(self, mock_discover, mock_set_raw):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({
                "credential": {"token": {"access_token": "token_switched"}}
            }, f)
            temp_path = f.name

        try:
            target = AgyProfile(id="switch_me", display_name="switch_me", short_name="switch_me", profile_path=temp_path)
            mock_discover.return_value = [target]
            mock_set_raw.return_value = True

            ok = switch_active_profile("switch_me")
            self.assertTrue(ok)
            mock_set_raw.assert_called_once_with({"token": {"access_token": "token_switched"}})

            # Switching to auto returns True immediately
            self.assertTrue(switch_active_profile("auto"))
        finally:
            if os.path.exists(temp_path):
                os.unlink(temp_path)

    @patch("core.providers.agy_provider.get_keyring_credential_raw")
    def test_save_current_as_profile(self, mock_get_raw):
        with tempfile.TemporaryDirectory() as tmp_dir:
            profiles_dir = os.path.join(tmp_dir, "profiles")
            with patch("core.providers.agy_provider.get_profiles_dir", return_value=profiles_dir):
                mock_get_raw.return_value = {
                    "token": {"access_token": "save_test_token"},
                    "email": "saved@gmail.com"
                }

                prof = save_current_as_profile("my_custom_alias")
                self.assertIsNotNone(prof)
                self.assertEqual(prof.id, "my_custom_alias")
                self.assertEqual(prof.short_name, "my_custom_alias")
                self.assertTrue(os.path.exists(prof.profile_path))

                with open(prof.profile_path, "r", encoding="utf-8") as f:
                    saved_data = json.load(f)
                self.assertEqual(saved_data["id"], "my_custom_alias")
                self.assertEqual(saved_data["credential"]["token"]["access_token"], "save_test_token")

    def test_agy_provider_display_name_with_profile(self):
        config_mock = MagicMock()
        config_mock.get.side_effect = lambda k, d=None: "mywork" if k == "agy_profile" else d

        provider = AgyProvider(config=config_mock)
        with patch("core.providers.agy_provider.resolve_active_profile") as mock_resolve:
            mock_resolve.return_value = (
                AgyProfile(id="mywork", display_name="mywork", short_name="mywork", is_keyring_active=False),
                False
            )
            # Test that metrics receives the formatted display name AGY (mywork)
            with patch.object(provider, "_get_access_token", return_value="fake_token"), \
                 patch.object(provider, "_fetch_via_http") as mock_http:
                mock_http.return_value = MagicMock(error=None, provider_name="AGY (mywork)")
                metrics = provider.fetch_usage()
                self.assertEqual(metrics.provider_name, "AGY (mywork)")


if __name__ == "__main__":
    unittest.main()
