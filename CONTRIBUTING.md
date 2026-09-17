# Contributing to AI HUD Monitor

Thank you for your interest in contributing to **AI HUD Monitor**! This project is open-source under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

To maintain high code quality, predictability, and stability, this project strictly adheres to professional open-source branch maintenance workflows and contribution standards.

---

## 🌳 Branch Maintenance Strategy (分支維護機制)

We follow an enhanced **Git Flow / GitHub Flow** branch lifecycle model:

```text
main (Protected, Stable Releases, vX.Y.Z tags)
  ▲
  │ (Release PR / Hotfix PR)
  ├─────────────────────────────────────────────┐
  │                                             │
develop (Integration Branch)             hotfix/vX.Y.Z
  ▲                                             ▲
  │ (Feature PR / Bugfix PR)                    │ (Critical bug)
  ├──────────────────────┬──────────────────────┤
feat/feature-name      fix/bug-name           main
```

### 1. Permanent Branches
* **`main` (Production & Releases)**:
  * **Strictly Protected**: Direct pushes are forbidden.
  * Represents official, battle-tested production releases.
  * Every merge into `main` must be tagged with SemVer (e.g., `v1.0.0`, `v1.1.0`).
  * Pushing a tag `v*` automatically triggers GitHub Actions to compile and publish standalone Windows `.exe` and macOS `.app` binaries.
* **`develop` (Active Integration)**:
  * Integration branch for the next upcoming release.
  * All feature PRs and non-urgent bug fixes merge here first.

### 2. Temporary / Topic Branches
* **`feat/<short-description>`**:
  * Branched from: `develop`
  * Merged back to: `develop`
  * Used for developing new features, providers, or UI components.
* **`fix/<issue-number>-<short-description>`**:
  * Branched from: `develop`
  * Merged back to: `develop`
  * Used for resolving bug reports.
* **`hotfix/v<version>`**:
  * Branched from: `main`
  * Merged back to: `main` AND `develop`
  * Used strictly for critical production bug fixes requiring immediate release.
* **`release/v<version>`**:
  * Branched from: `develop`
  * Used for version bumping, final changelog curation, and regression testing before merging to `main`.

---

## 📝 Commit Message Conventions (Conventional Commits)

We enforce the [Conventional Commits v1.0.0](https://www.conventionalcommits.org/) specification. Each commit message must follow this structure:

```text
<type>(<scope>): <subject>

[optional body]

[optional footer(s)]
```

### Types:
* `feat`: A new feature (e.g., `feat(provider): add deepseek usage monitor`).
* `fix`: A bug fix (e.g., `fix(codex): correct WHAM primary window rate limit parsing`).
* `docs`: Documentation only changes (e.g., `docs(readme): add macOS installation guide`).
* `style`: Changes that do not affect code logic (formatting, white-space, etc.).
* `refactor`: Code changes that neither fix a bug nor add a feature.
* `perf`: A code change that improves performance.
* `test`: Adding missing tests or correcting existing tests.
* `chore`: Build scripts, CI workflow, or dependency bumps (e.g., `chore(ci): update PySide6 build matrix`).

---

## 🚀 Development Setup

1. **Fork and clone the repository**:
   ```bash
   git clone https://github.com/<your-username>/claude-hud-monitor.git
   cd claude-hud-monitor
   ```

2. **Create and activate a virtual environment**:
   ```bash
   python -m venv .venv
   # Windows PowerShell:
   .venv\Scripts\Activate.ps1
   # macOS / Linux:
   source .venv/bin/activate
   ```

3. **Install dependencies**:
   ```bash
   python -m pip install -r requirements-build.txt
   ```

4. **Create a topic branch**:
   ```bash
   git checkout -b feat/my-new-feature develop
   ```

5. **Run and verify locally**:
   ```bash
   python -B -m unittest discover -s tests -v
   python main.py
   ```

---

## 📬 Pull Request (PR) Process

1. Run `python -B -m unittest discover -s tests -v` locally. Tests must use synthetic data and temporary settings, never real credentials.
2. Submit your PR targeting the **`develop`** branch (not `main`).
3. Fill out the [Pull Request Template](.github/PULL_REQUEST_TEMPLATE.md) completely.
4. Link the relevant GitHub Issue in your PR description (e.g., `Fixes #12`).
5. Maintainers will review your PR, run CI checks, and request changes if necessary.
6. Once approved, your PR will be squashed and merged into `develop`.

---

## ⚖️ AGPL-3.0 License & Inbound Rights

By contributing to **AI HUD Monitor**, you acknowledge and agree that:
* Your contributions will be licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.
* Any network-deployed or client-distributed modifications must make their source code available under AGPL-3.0.
* You hold the necessary rights to submit the code.

## Local validation before release

Keep fixes on a local topic branch until reviewed. Do not push tags or publish during local validation.
Existing build job names are retained for branch-protection compatibility. The workflow runs regression tests before packaging on PRs and pushes.
Runtime/build requirements are centralized. macOS-only pynput and its platform dependencies still use version ranges and require macOS validation; this is not a fully hash-locked environment.
See [architecture](PROJECT_SPEC.md), [provider compatibility](docs/PROVIDERS.md), and [validation record](docs/LOCAL_VALIDATION.md).
