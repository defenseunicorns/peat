# Peat Whitepaper Build System

This directory contains the source files and build system for the Peat Protocol whitepaper.

## Structure

```
whitepaper/
├── README.md              # This file
├── Makefile               # Build system
├── metadata.yaml          # Document metadata (title, author, etc.)
├── 00-front-matter.md     # Title page
├── 01-executive-summary.md # Executive summary
├── 02-scaling-crisis.md   # Section I: The Scaling Crisis
├── 03-standards-paradox.md # Section II: The Standards Paradox
├── 04-hierarchy-insight.md # Section III: The Hierarchy Insight
├── 05-technical-architecture.md # Section IV: Technical Architecture
├── 06-open-architecture.md # Section V: Open Architecture Imperative
├── 07-why-now.md          # Section VI: Why Now
├── 08-path-forward.md     # Section VII: Path Forward
├── 09-conclusion.md       # Conclusion
├── 10-appendices.md       # Appendices
├── templates/             # Output templates
│   ├── html.template      # HTML template
│   ├── latex.template     # PDF/LaTeX template
│   └── style.css          # HTML styles
└── build/                 # Generated output (gitignored)
    ├── Peat_Whitepaper.html
    └── Peat_Whitepaper.pdf
```

## Prerequisites

Install required tools:

```bash
# Check what's needed
make check-deps

# Install pandoc (required)
brew install pandoc

# Install LaTeX for PDF generation (optional but recommended)
brew install --cask mactex-no-gui  # ~4GB, full distribution
# OR: brew install --cask basictex  # ~100MB, minimal (may need tlmgr for packages)

# Install entr for live rebuilding (optional)
brew install entr
```

## Building

```bash
# Build both HTML and PDF
make all

# Build HTML only (faster, no LaTeX needed)
make html

# Build PDF only
make pdf

# Build Word document (for collaboration)
make docx

# Clean generated files
make clean

# Watch for changes and rebuild HTML
make watch

# Show word count by section
make wordcount
```

## Output

Generated files are placed in `build/`:
- `Peat_Whitepaper.html` - Self-contained HTML with embedded styles
- `Peat_Whitepaper.pdf` - Professional PDF document
- `Peat_Whitepaper.docx` - Word document (if built)

## Writing Content

Each section file uses standard Markdown with:
- `##` for major sections (becomes Section I, II, etc.)
- `###` for subsections (1.1, 1.2, etc.)
- `####` for sub-subsections
- HTML comments `<!-- TODO: ... -->` for notes
- Blockquotes `>` for Key Findings

### Content TODO Markers

Content placeholders are marked with HTML comments:
```markdown
<!-- TODO: Content to develop:
- Point 1
- Point 2
-->
```

### Key Findings Format

Each section ends with a Key Finding blockquote:
```markdown
### Key Finding: Section I

> "The ~20 platform ceiling isn't a technology gap—it's an architecture gap..."
```

## Customization

### Metadata
Edit `metadata.yaml` to change:
- Title, subtitle, author info
- Abstract and keywords
- PDF formatting options
- Classification markings

### Styles
- HTML: Edit `templates/style.css`
- PDF: Edit `templates/latex.template`

## Adding Diagrams

Place images in an `images/` directory and reference them:
```markdown
![Scaling curve](images/scaling-curve.png)
```

For diagrams, consider:
- [Mermaid](https://mermaid.js.org/) - text-based diagrams
- [Draw.io](https://draw.io) - visual editor, export as PNG/SVG
- [Excalidraw](https://excalidraw.com/) - hand-drawn style

## Version Control

The `build/` directory is gitignored. Commit source `.md` files, not generated output.

To create a release:
1. Build final versions: `make all`
2. Copy from `build/` to release location
3. Tag the commit: `git tag -a v0.1 -m "Whitepaper v0.1"`
