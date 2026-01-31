# HIVE Whitepaper Build System
# Requires: pandoc, typst (for PDF)
#
# Usage:
#   make html    - Build HTML version
#   make pdf     - Build PDF version (via Typst - recommended)
#   make pdf-latex - Build PDF via LaTeX (legacy)
#   make all     - Build both HTML and PDF
#   make clean   - Remove generated files
#   make watch   - Watch for changes and rebuild (requires entr)

# Configuration
OUTPUT_DIR = build
OUTPUT_NAME = HIVE_Whitepaper
METADATA = metadata.yaml

# Source files in order (for HTML build)
SOURCES = \
	00-front-matter.md \
	01-executive-summary.md \
	02-scaling-crisis.md \
	03-standards-paradox.md \
	04-hierarchy-insight.md \
	05-technical-architecture.md \
	06-open-architecture.md \
	07-why-now.md \
	08-path-forward.md \
	09-conclusion.md \
	10a-appendix-a.md \
	10b-spec-appendix.md \
	10c-appendix-bc.md \
	11-adr-appendix.md \
	12-glossary.md

# Pandoc options
PANDOC_COMMON = --from markdown \
	--metadata-file=$(METADATA) \
	--standalone \
	--table-of-contents \
	--toc-depth=3 \
	--number-sections

PANDOC_HTML = $(PANDOC_COMMON) \
	--to html5 \
	--include-in-header=templates/style-header.html \
	--embed-resources

PANDOC_PDF = $(PANDOC_COMMON) \
	--to pdf \
	--pdf-engine=xelatex \
	--template=templates/latex.template \
	-V geometry:margin=1in \
	-V fontsize=11pt \
	-V documentclass=article \
	-V colorlinks=true \
	-V linkcolor=NavyBlue \
	-V urlcolor=NavyBlue

# Default target
.PHONY: all
all: html pdf

# Create output directory
$(OUTPUT_DIR):
	mkdir -p $(OUTPUT_DIR)

# Build HTML
.PHONY: html
html: $(OUTPUT_DIR) templates/html.template templates/style.css
	pandoc $(PANDOC_HTML) -o $(OUTPUT_DIR)/$(OUTPUT_NAME).html $(SOURCES)
	@echo "Built: $(OUTPUT_DIR)/$(OUTPUT_NAME).html"

# Sources for PDF (main content + appendix headers, specs/ADRs processed separately)
SOURCES_MAIN = \
	00-front-matter.md \
	01-executive-summary.md \
	02-scaling-crisis.md \
	03-standards-paradox.md \
	04-hierarchy-insight.md \
	05-technical-architecture.md \
	06-open-architecture.md \
	07-why-now.md \
	08-path-forward.md \
	09-conclusion.md \
	10a-appendix-a.md

# Build PDF via Typst (recommended - native unicode support)
.PHONY: pdf
pdf: $(OUTPUT_DIR)
	@echo "Converting Markdown to Typst..."
	@cat templates/preamble.typ > $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@# Process main content + Appendix A header
	@pandoc --from markdown --to typst \
		$(SOURCES_MAIN) >> $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@# Process spec content: Spec titles (# HIVE Protocol Specification) -> #### (in TOC under Appendix A)
	@sed -E 's/^#### /###### /; s/^### /###### /; s/^## /##### /; s/^# (HIVE Protocol Specification)/#### \1/; s/^# /##### /' 10b-spec-appendix.md | \
		pandoc --from markdown --to typst >> $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@# Process Appendix B content + Appendix C header
	@pandoc --from markdown --to typst 10c-appendix-bc.md >> $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@# Process ADR content: Only ADR titles (# ADR) -> #### (in TOC under Appendix C)
	@sed -E 's/^#### /###### /; s/^### /###### /; s/^## /##### /; s/^# (ADR)/#### \1/; s/^# /##### /' 11-adr-appendix.md | \
		pandoc --from markdown --to typst >> $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@# Process Appendix D: Glossary (comes after ADRs)
	@pandoc --from markdown --to typst 12-glossary.md >> $(OUTPUT_DIR)/$(OUTPUT_NAME).typ
	@echo "Compiling Typst to PDF..."
	@typst compile $(OUTPUT_DIR)/$(OUTPUT_NAME).typ $(OUTPUT_DIR)/$(OUTPUT_NAME).pdf
	@echo "Built: $(OUTPUT_DIR)/$(OUTPUT_NAME).pdf (via Typst)"

# Build PDF via LaTeX (legacy)
.PHONY: pdf-latex
pdf-latex: $(OUTPUT_DIR) templates/latex.template
	pandoc $(PANDOC_PDF) -o $(OUTPUT_DIR)/$(OUTPUT_NAME).pdf $(SOURCES)
	@echo "Built: $(OUTPUT_DIR)/$(OUTPUT_NAME).pdf (via LaTeX)"

# Build Word document (useful for sharing/collaboration)
.PHONY: docx
docx: $(OUTPUT_DIR)
	pandoc $(PANDOC_COMMON) \
		--to docx \
		--reference-doc=templates/reference.docx \
		-o $(OUTPUT_DIR)/$(OUTPUT_NAME).docx $(SOURCES)
	@echo "Built: $(OUTPUT_DIR)/$(OUTPUT_NAME).docx"

# Clean generated files
.PHONY: clean
clean:
	rm -rf $(OUTPUT_DIR)

# Watch for changes and rebuild (requires entr: brew install entr)
.PHONY: watch
watch:
	@echo "Watching for changes... (Ctrl+C to stop)"
	@find . -name '*.md' -o -name '*.yaml' -o -name '*.css' -o -name '*.template' | \
		entr -c make html

# Watch and rebuild PDF
.PHONY: watch-pdf
watch-pdf:
	@echo "Watching for changes... (Ctrl+C to stop)"
	@find . -name '*.md' -o -name '*.yaml' -o -name '*.template' | \
		entr -c make pdf

# Check dependencies
.PHONY: check-deps
check-deps:
	@echo "Checking dependencies..."
	@which pandoc > /dev/null || (echo "ERROR: pandoc not found. Install with: brew install pandoc" && exit 1)
	@echo "  pandoc: $$(pandoc --version | head -1)"
	@which xelatex > /dev/null || echo "  WARNING: xelatex not found. PDF generation requires: brew install --cask mactex-no-gui"
	@which entr > /dev/null || echo "  NOTE: entr not found (optional, for 'make watch'). Install with: brew install entr"
	@echo "Dependency check complete."

# Word count
.PHONY: wordcount
wordcount:
	@echo "Word count by section:"
	@for f in $(SOURCES); do \
		wc -w < "$$f" | tr -d ' ' | xargs printf "  %5s words: %s\n" ; \
	done | sed 's|: |: |'
	@echo "---"
	@echo "Total: $$(cat $(SOURCES) | wc -w | tr -d ' ') words"

# Help
.PHONY: help
help:
	@echo "HIVE Whitepaper Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make all        - Build HTML and PDF versions"
	@echo "  make html       - Build HTML version only"
	@echo "  make pdf        - Build PDF version only"
	@echo "  make docx       - Build Word document"
	@echo "  make clean      - Remove generated files"
	@echo "  make watch      - Watch for changes and rebuild HTML"
	@echo "  make watch-pdf  - Watch for changes and rebuild PDF"
	@echo "  make wordcount  - Show word count by section"
	@echo "  make check-deps - Check for required dependencies"
	@echo "  make help       - Show this help"
	@echo ""
	@echo "Output directory: $(OUTPUT_DIR)/"
