from inferqos_client import headers, interactive
def test_scope_restores_headers():
    with interactive(250) as value:
        assert value["X-InferQoS-Deadline-Ms"] == "250"
        assert headers()["X-InferQoS-Class"] == "interactive"
    assert headers() == {}

