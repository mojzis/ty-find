.PHONY: review fmt-check lint test audit deny coverage mutants review-quick docs docs-serve docs-clean

# Full review — run before pushing or merging
review: fmt-check lint test audit deny
	@echo ""
	@echo "✅ All review checks passed"

# Quick review — skip slower network checks
review-quick: fmt-check lint test
	@echo ""
	@echo "✅ Quick review passed"

fmt-check:
	@echo "📐 Checking formatting..."
	@cargo fmt --all -- --check

lint:
	@echo "🔍 Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings

test:
	@echo "🧪 Running tests..."
	@if command -v cargo-nextest > /dev/null 2>&1; then \
		cargo nextest run --all-features; \
	else \
		cargo test --all-features; \
	fi

audit:
	@echo "🔒 Running security audit..."
	@if command -v cargo-audit > /dev/null 2>&1; then \
		cargo audit; \
	else \
		echo "⚠️  cargo-audit not installed. Run: cargo install cargo-audit"; \
	fi

deny:
	@echo "🚫 Checking dependency policies..."
	@if command -v cargo-deny > /dev/null 2>&1; then \
		cargo deny check; \
	else \
		echo "⚠️  cargo-deny not installed. Run: cargo install cargo-deny"; \
	fi

coverage:
	@echo "📊 Generating coverage report..."
	@if command -v cargo-llvm-cov > /dev/null 2>&1; then \
		cargo llvm-cov --all-features --workspace --html; \
		echo "Report: target/llvm-cov/html/index.html"; \
	else \
		echo "⚠️  cargo-llvm-cov not installed. Run: cargo install cargo-llvm-cov"; \
	fi

mutants:
	@echo "🧬 Running mutation testing on recent changes..."
	@if command -v cargo-mutants > /dev/null 2>&1; then \
		cargo mutants --in-diff HEAD~1..HEAD; \
	else \
		echo "⚠️  cargo-mutants not installed. Run: cargo install cargo-mutants"; \
	fi

docs:
	@echo "📖 Building documentation..."
	@if command -v mdbook > /dev/null 2>&1; then \
		mdbook build docs; \
		bash docs/generate-llms-txt.sh; \
		echo "Docs built at docs/book/html/index.html"; \
	else \
		echo "⚠️  mdbook not installed. Run: cargo install mdbook"; \
	fi

docs-serve:
	@echo "📖 Serving documentation with live reload..."
	@if command -v mdbook > /dev/null 2>&1; then \
		mdbook serve docs --open; \
	else \
		echo "⚠️  mdbook not installed. Run: cargo install mdbook"; \
	fi

docs-clean:
	@echo "🧹 Cleaning built documentation..."
	@rm -rf docs/book
	@echo "Cleaned docs/book/"
