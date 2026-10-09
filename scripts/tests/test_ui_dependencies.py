import copy
import importlib.util
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location(
    "check_ui_dependencies", Path(__file__).resolve().parents[1] / "check_ui_dependencies.py"
)
check_ui_dependencies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_ui_dependencies)


class UiDependencyTests(unittest.TestCase):
    def setUp(self):
        versions = {"postman-gpui": "0.1.0", "postman-cli": "0.1.0", "postman-http": "0.1.0",
                    "gpui-kit": "0.7.1", "gpui-pre": "0.3.8", "gpui-pre-platform": "0.3.8",
                    "gpui-pre-macros": "0.3.8"}
        dependencies = {"postman-gpui": ["gpui-kit", "postman-http"],
                        "postman-cli": ["postman-http"], "postman-http": [],
                        "gpui-kit": ["gpui-pre", "gpui-pre-platform"],
                        "gpui-pre": ["gpui-pre-macros"], "gpui-pre-platform": ["gpui-pre"],
                        "gpui-pre-macros": []}
        self.metadata = {
            "packages": [{"id": name, "name": name, "version": version,
                          "source": "registry+https://github.com/rust-lang/crates.io-index"}
                         for name, version in versions.items()],
            "workspace_members": ["postman-gpui", "postman-cli", "postman-http"],
            "resolve": {"nodes": [{"id": name, "dependencies": deps}
                                  for name, deps in dependencies.items()]},
        }

    def test_accepts_one_pinned_stack_and_headless_engines(self):
        self.assertEqual(check_ui_dependencies.check(self.metadata), [])

    def test_rejects_transitive_gui_dependency_in_a_headless_crate(self):
        self.metadata["resolve"]["nodes"][2]["dependencies"].append("gpui-kit")
        errors = check_ui_dependencies.check(self.metadata)
        self.assertTrue(any("postman-cli must remain headless" in error for error in errors))
        self.assertTrue(any("postman-http must remain headless" in error for error in errors))

    def test_rejects_a_second_gpui_version(self):
        package = copy.deepcopy(self.metadata["packages"][4])
        package.update(id="gpui-pre-other", version="0.3.9")
        self.metadata["packages"].append(package)
        self.assertTrue(any("Expected one gpui-pre" in error
                            for error in check_ui_dependencies.check(self.metadata)))

    def test_rejects_git_replacement_and_legacy_gpui(self):
        self.metadata["packages"][4]["source"] = "git+https://example.test/gpui"
        self.metadata["packages"].append({"id": "old-gpui", "name": "gpui", "version": "0.2.0"})
        errors = check_ui_dependencies.check(self.metadata)
        self.assertTrue(any("crates.io release" in error for error in errors))
        self.assertTrue(any("second GPUI stack" in error for error in errors))
