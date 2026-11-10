"""Regression checks for project-owned backend recovery."""
import fcntl
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import Mock, patch

LAUNCHER = runpy.run_path(str(Path(__file__).with_name('launch-client.py')))


class StopLoop(Exception):
    pass


class RecoveryTests(unittest.TestCase):
    def test_retries_after_failure_and_keeps_monitoring(self):
        start = Mock(side_effect=[RuntimeError('adapter stopped'), None, None])
        sleep = Mock(side_effect=[None, StopLoop])
        function = LAUNCHER['start_backend']
        with tempfile.TemporaryDirectory() as directory:
            with patch.dict(function.__globals__, PRIVATE=Path(directory), start=start), patch('time.sleep', sleep):
                with self.assertRaises(StopLoop):
                    function(supervise=True)
        self.assertEqual(start.call_count, 3)
        self.assertEqual(sleep.call_count, 2)

    def test_second_supervisor_does_not_start_duplicate_backends(self):
        function = LAUNCHER['start_backend']
        start = Mock()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (root / 'desktop-start.lock').open('a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with patch.dict(function.__globals__, PRIVATE=root, start=start):
                    function(supervise=True)
        start.assert_not_called()


if __name__ == '__main__':
    unittest.main()
