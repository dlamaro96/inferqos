import unittest

from inferqos_client import headers, interactive


class ClientTests(unittest.TestCase):
    def test_scope_restores_headers(self):
        with interactive(250) as value:
            self.assertEqual(value["X-InferQoS-Deadline-Ms"], "250")
            self.assertEqual(headers()["X-InferQoS-Class"], "interactive")
        self.assertEqual(headers(), {})


if __name__ == "__main__":
    unittest.main()
