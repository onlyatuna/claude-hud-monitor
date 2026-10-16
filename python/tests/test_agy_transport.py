import json
import unittest
from datetime import datetime, timedelta, timezone
from unittest.mock import patch
from urllib.error import HTTPError
from core.providers.agy_provider import AgyProvider

SUMMARY_FIXTURE = (
    b'{"groups":[{"name":"Claude and GPT models","buckets":['
    b'{"id":"3p-weekly","remainingFraction":0.8449,"resetTime":"2026-10-23T12:00:00Z"},'
    b'{"id":"3p-5h","remainingFraction":0.694,"resetTime":"2026-10-16T14:00:00Z"}'
    b']}]}'
)

QUOTA_FIXTURE = (
    b'{"buckets":['
    b'{"modelId":"gemini-3.8-flash-tiered","remainingFraction":0.9743,"resetTime":"2026-10-16T13:41:00Z","tokenType":"WTUS"},'
    b'{"modelId":"gemini-3.8-flash-low","remainingFraction":0.9743,"resetTime":"2026-10-16T13:41:00Z"},'
    b'{"modelId":"claude-sonnet-5-5-high","remainingFraction":0.694,"resetTime":"2026-10-16T14:00:00Z"}'
    b']}'
)


class _MockResponse:
    def __init__(self, body: bytes, status: int = 200):
        self.body = body
        self.status = status

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self):
        return self.body


class AgyTransportTests(unittest.TestCase):
    def setUp(self):
        cache_patch = patch("core.providers.agy_provider.read_quota_cache", return_value=None, create=True)
        self.cache_reader = cache_patch.start()
        self.addCleanup(cache_patch.stop)
        reset = datetime.now(timezone.utc) + timedelta(days=7)
        self.cache = {
            "written_at": datetime.now(timezone.utc),
            "buckets": {
                key: {"remaining_fraction": fraction, "reset_time": reset}
                for key, fraction in {
                    "gemini-5h": 1.0, "gemini-weekly": 0.9914498,
                    "3p-5h": 1.0, "3p-weekly": 0.8469924,
                    "gemini-argon-wtu-weekly": 0.78630334,
                }.items()
            },
        }

    def test_cache_complements_http_with_rest_priority(self):
        self.cache_reader.return_value = self.cache
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value="token"), patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[_MockResponse(SUMMARY_FIXTURE), _MockResponse(QUOTA_FIXTURE)],
        ):
            result = provider.fetch_usage()
        self.assertIsNone(result.error)
        self.assertEqual(result.error_code, "")
        self.assertAlmostEqual(result.metric1_val, 2.57)
        self.assertAlmostEqual(result.extra_metrics[0].val, 30.6)
        self.assertEqual(result.extra_metrics[0].reset.isoformat(), "2026-10-16T14:00:00+00:00")
        self.assertAlmostEqual(result.metric2_val, 0.85502)
        self.assertEqual(result.metric2_reset, self.cache["buckets"]["gemini-weekly"]["reset_time"])
        self.assertEqual([r.title for r in result.extra_metrics], ["C/G 5H", "C/G 7D", "ARGON 7D"])
        self.assertAlmostEqual(result.extra_metrics[2].val, 21.369666)
        self.assertAlmostEqual(result.extra_metrics[1].val, 15.51)
        self.assertEqual(result.metric1_title, "GEMINI 5H")
        self.assertEqual(result.metric2_title, "GEMINI 7D")
        self.assertEqual(result.badge1_text, "C/G 剩餘: 84%")
        self.assertEqual(result.badge2_text, "Argon 剩餘: 79%")

    def test_cache_without_token_provides_complete_metrics(self):
        self.cache_reader.return_value = self.cache
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None):
            result = provider.fetch_usage()
        self.assertIsNone(result.error)
        self.assertEqual(result.error_code, "")
        self.assertEqual(result.metric1_val, 0.0)
        self.assertEqual(result.metric1_text, "0%")
        self.assertAlmostEqual(result.extra_metrics[1].val, 15.30076)
        self.assertEqual(result.metric1_reset, self.cache["buckets"]["gemini-5h"]["reset_time"])
        self.assertAlmostEqual(result.metric2_val, 0.85502)
        self.assertEqual(result.badge1_text, "C/G 剩餘: 85%")
        self.assertEqual(result.badge2_text, "Argon 剩餘: 79%")

    def test_cache_recovers_when_all_http_requests_fail(self):
        self.cache_reader.return_value = self.cache
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value="token"), patch(
            "core.providers.agy_provider.urllib.request.urlopen", side_effect=OSError("offline")
        ):
            result = provider.fetch_usage()
        self.assertIsNone(result.error)
        self.assertEqual(result.error_code, "")
        self.assertEqual(result.metric1_val, 0.0)
        self.assertAlmostEqual(result.metric2_val, 0.85502)
        self.assertEqual(result.badge1_text, "C/G 剩餘: 85%")
        self.assertEqual(result.badge2_text, "Argon 剩餘: 79%")

    def test_missing_weekly_bucket_requests_statusline(self):
        del self.cache["buckets"]["gemini-weekly"]
        self.cache_reader.return_value = self.cache
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None):
            result = provider.fetch_usage()
        self.assertIsNone(result.metric2_val)
        self.assertEqual(result.metric2_text, "--")
        self.assertEqual(result.metric2_subtext, "需啟用 agy statusLine")

    def test_cache_exception_preserves_no_token_error(self):
        self.cache_reader.side_effect = OSError("unreadable")
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None):
            result = provider.fetch_usage()
        self.assertEqual(result.error_code, "no_token")
        self.assertEqual(result.badge2_text, "Argon: --")
        if result.error:
            self.assertEqual(result.extra_metrics, [])
        else:
            self.assertIsNone(result.extra_metrics[2].val)
            self.assertEqual(result.extra_metrics[2].subtext, "需啟用 agy statusLine")

    def test_no_cache_preserves_http_auth_error(self):
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value="token"), patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=HTTPError(provider.SUMMARY_URL, 401, "unauthorized", {}, None),
        ):
            result = provider.fetch_usage()
        self.assertEqual(result.error_code, "auth")
        self.assertEqual(result.badge2_text, "Argon: --")
        if result.error:
            self.assertEqual(result.extra_metrics, [])
        else:
            self.assertIsNone(result.extra_metrics[2].val)
            self.assertEqual(result.extra_metrics[2].subtext, "需啟用 agy statusLine")

    def test_empty_cache_returns_schema_error(self):
        self.cache_reader.return_value = {"buckets": {}}
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None):
            result = provider.fetch_usage()
        self.assertEqual(result.error_code, "schema")
        self.assertEqual(result.extra_metrics, [])
        self.assertEqual(result.error, "未取得有效配額資料")

    def test_both_endpoints_succeed_and_request_spec(self):
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value="token"), patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[_MockResponse(SUMMARY_FIXTURE), _MockResponse(QUOTA_FIXTURE)],
        ) as urlopen:
            result = provider.fetch_usage()

        self.assertIsNone(result.error)
        self.assertAlmostEqual(result.metric1_val, 2.57)
        self.assertAlmostEqual(result.extra_metrics[0].val, 30.6)
        self.assertEqual(result.extra_metrics[0].reset.isoformat(), "2026-10-16T14:00:00+00:00")
        self.assertEqual(result.metric1_text, "3%")
        self.assertEqual(result.badge1_text, "C/G 剩餘: 84%")
        self.assertEqual(result.badge2_text, "Argon: --")
        if result.error:
            self.assertEqual(result.extra_metrics, [])
        else:
            self.assertIsNone(result.extra_metrics[2].val)
            self.assertEqual(result.extra_metrics[2].subtext, "需啟用 agy statusLine")
        self.assertIsNone(result.metric2_val)
        self.assertEqual(result.metric2_text, "--")
        self.assertEqual(result.metric2_subtext, "Gemini 週額度來源未知")

        requests = [call.args[0] for call in urlopen.call_args_list]
        self.assertEqual([req.full_url for req in requests], [provider.SUMMARY_URL, provider.QUOTA_URL])
        for req in requests:
            self.assertEqual(req.get_method(), "POST")
            self.assertEqual(req.data, b"{}")
            self.assertEqual(req.get_header("Authorization"), "Bearer token")

    def test_gemini_picks_most_constrained_bucket(self):
        quota_payload = json.dumps(
            {
                "buckets": [
                    {"modelId": "gemini-3.8-flash-low", "remainingFraction": 0.9, "resetTime": "2026-10-16T13:00:00Z"},
                    {"modelId": "gemini-3.8-pro-high", "remainingFraction": 0.25, "resetTime": "2026-10-16T15:00:00Z"},
                ]
            }
        ).encode("utf-8")
        provider = AgyProvider()
        with patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[_MockResponse(SUMMARY_FIXTURE), _MockResponse(quota_payload)],
        ):
            result = provider._fetch_via_http("token", "12:00:00")

        self.assertIsNone(result.error)
        self.assertAlmostEqual(result.metric1_val, 75.0)
        self.assertEqual(result.metric1_text, "75%")
        self.assertEqual(result.metric1_reset.isoformat(), "2026-10-16T15:00:00+00:00")

    def test_unknown_group_logged_and_ignored(self):
        summary_with_unknown = json.dumps(
            {
                "groups": [
                    {
                        "name": "Claude and GPT models",
                        "buckets": [{"id": "3p-weekly", "remainingFraction": 0.8449, "resetTime": "2026-10-23T12:00:00Z"}],
                    },
                    {
                        "name": "Gemini 4 Argon",
                        "buckets": [{"id": "argon-5h", "remainingFraction": 0.1}],
                    },
                ]
            }
        ).encode("utf-8")
        provider = AgyProvider()
        with patch("core.providers.agy_provider.logger") as mock_logger, patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[_MockResponse(summary_with_unknown), _MockResponse(QUOTA_FIXTURE)],
        ):
            result = provider._fetch_via_http("token", "12:00:00")

        self.assertIsNone(result.error)
        self.assertAlmostEqual(result.metric1_val, 2.57)
        self.assertIsNone(result.extra_metrics[0].val)
        self.assertEqual(result.badge1_text, "C/G 剩餘: 84%")
        mock_logger.info.assert_called()
        logged_msgs = [str(call.args[0]) for call in mock_logger.info.call_args_list]
        self.assertTrue(any("Gemini 4 Argon" in msg for msg in logged_msgs))

    def test_endpoint_a_401_returns_auth_error(self):
        provider = AgyProvider()
        with patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=HTTPError(provider.SUMMARY_URL, 401, "unauthorized", {}, None),
        ) as urlopen:
            result = provider._fetch_via_http("token", "12:00:00")

        self.assertEqual(result.error_code, "auth")
        self.assertIsNotNone(result.error)
        self.assertEqual(urlopen.call_count, 1)

    def test_endpoint_b_failure_returns_partial_result(self):
        provider = AgyProvider()
        with patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[
                _MockResponse(SUMMARY_FIXTURE),
                HTTPError(provider.QUOTA_URL, 503, "down", {}, None),
            ],
        ):
            result = provider._fetch_via_http("token", "12:00:00")

        self.assertIsNone(result.error)
        self.assertEqual(result.badge1_text, "C/G 剩餘: 84%")
        self.assertIsNone(result.metric1_val)
        self.assertEqual(result.metric1_text, "--")

    def test_both_endpoints_fail_returns_error(self):
        provider = AgyProvider()
        with patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[
                HTTPError(provider.SUMMARY_URL, 503, "down", {}, None),
                HTTPError(provider.QUOTA_URL, 503, "down", {}, None),
            ],
        ):
            result = provider._fetch_via_http("token", "12:00:00")

        self.assertIsNotNone(result.error)
        self.assertEqual(result.error_code, "schema")
        self.assertEqual(result.extra_metrics, [])

    def test_no_token_returns_no_token_without_subprocess(self):
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None), patch(
            "core.providers.agy_provider.subprocess.run"
        ) as mock_run:
            result = provider.fetch_usage()

        self.assertEqual(result.error_code, "no_token")
        self.assertIsNotNone(result.error)
        mock_run.assert_not_called()
