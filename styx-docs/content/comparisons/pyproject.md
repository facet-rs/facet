+++
title = "pyproject.toml"
weight = 10
slug = "pyproject"
insert_anchor_links = "heading"
+++

A Python pyproject.toml in TOML vs Styx.

```compare
/// toml
[project]
name = "mypackage"
version = "1.0.0"
description = "A Python package"
readme = "README.md"
license = { text = "MIT" }
authors = [
    { name = "Alice", email = "alice@example.com" }
]
requires-python = ">=3.11"
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: MIT License",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
]
dependencies = [
    "httpx>=0.25.0",
    "pydantic>=2.0.0",
    "rich>=13.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0.0",
    "pytest-cov>=4.0.0",
    "mypy>=1.0.0",
    "ruff>=0.1.0",
]
docs = [
    "mkdocs>=1.5.0",
    "mkdocs-material>=9.0.0",
]

[project.scripts]
mypackage = "mypackage.cli:main"

[project.urls]
Homepage = "https://github.com/example/mypackage"
Documentation = "https://mypackage.readthedocs.io"
Repository = "https://github.com/example/mypackage.git"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff]
line-length = 100
target-version = "py311"

[tool.ruff.lint]
select = ["E", "F", "I", "N", "W", "UP"]
ignore = ["E501"]

[tool.mypy]
python_version = "3.11"
strict = true
warn_return_any = true
warn_unused_ignores = true

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "-ra -q --cov=mypackage"
/// styx
project {
  name mypackage
  version 1.0.0
  description "A Python package"
  readme README.md
  license text>MIT
  authors ({name Alice, email alice@example.com})
  requires-python ">=3.11"

  classifiers (
    "Development Status :: 4 - Beta"
    "Intended Audience :: Developers"
    "License :: OSI Approved :: MIT License"
    "Programming Language :: Python :: 3.11"
    "Programming Language :: Python :: 3.12"
  )

  dependencies (
    httpx>=0.25.0
    pydantic>=2.0.0
    rich>=13.0.0
  )

  optional-dependencies {
    dev (pytest>=7.0.0 pytest-cov>=4.0.0 mypy>=1.0.0 ruff>=0.1.0)
    docs (mkdocs>=1.5.0 mkdocs-material>=9.0.0)
  }

  scripts mypackage>mypackage.cli:main

  urls {
    Homepage https://github.com/example/mypackage
    Documentation https://mypackage.readthedocs.io
    Repository https://github.com/example/mypackage.git
  }
}

build-system {
  requires (hatchling)
  build-backend hatchling.build
}

tool.ruff {
  line-length 100
  target-version py311
  lint {
    select (E F I N W UP)
    ignore (E501)
  }
}

tool.mypy {
  python_version 3.11
  strict true
  warn_return_any true
  warn_unused_ignores true
}

tool.pytest.ini_options {
  testpaths (tests)
  addopts "-ra -q --cov=mypackage"
}
```
