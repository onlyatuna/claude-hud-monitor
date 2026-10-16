import json
import unittest
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
    def test_both_endpoints_succeed_and_request_spec(self):
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value="token"), patch(
            "core.providers.agy_provider.urllib.request.urlopen",
            side_effect=[_MockResponse(SUMMARY_FIXTURE), _MockResponse(QUOTA_FIXTURE)],
        ) as urlopen:
            result = provider.fetch_usage()

        self.assertIsNone(result.error)
        self.assertAlmostEqual(result.metric1_val, 2.57)
        self.assertEqual(result.metric1_text, "3%")
        self.assertEqual(result.badge1_text, "C/G 剩餘: 84%")
        self.assertEqual(result.badge2_text, "Gemini Models")
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

    def test_no_token_returns_no_token_without_subprocess(self):
        provider = AgyProvider()
        with patch.object(provider, "_get_access_token", return_value=None), patch(
            "core.providers.agy_provider.subprocess.run"
        ) as mock_run:
            result = provider.fetch_usage()

        self.assertEqual(result.error_code, "no_token")
        self.assertIsNotNone(result.error)
        mock_run.assert_not_called()
