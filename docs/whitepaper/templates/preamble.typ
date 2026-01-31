// HIVE Whitepaper Typst Preamble
// Defines functions needed by pandoc's typst output

// Colors
#let primary-blue = rgb("#0066CC")
#let accent-blue = rgb("#3D8BFF")
#let highlight-blue = rgb("#60A5FA")
#let text-muted = rgb("#6B7280")

// Page setup - maximum density
#set page(
  paper: "us-letter",
  margin: (x: 0.6in, y: 0.5in),
  header: context {
    if counter(page).get().first() > 1 [
      #set text(size: 7pt, fill: text-muted)
      HIVE Protocol: Breaking the 20-Node Wall
      #h(1fr)
      v1.0
    ]
  },
  footer: context {
    set text(size: 7pt, fill: text-muted)
    h(1fr)
    counter(page).display()
    h(1fr)
  },
)

// Typography - maximum density with paragraph spacing
#set text(font: "Helvetica Neue", size: 9pt)
#set par(justify: true, leading: 0.5em, spacing: 1.2em, first-line-indent: 1em)

// Headings - good separation above and below
#show heading.where(level: 1): it => {
  set text(fill: primary-blue, size: 12pt, weight: "bold")
  block(above: 1.2em, below: 0.8em)[
    #it
    #v(-0.2em)
    #line(length: 100%, stroke: 0.5pt + primary-blue)
  ]
}
#show heading.where(level: 2): it => {
  set text(fill: accent-blue, size: 10pt, weight: "bold")
  block(above: 1.2em, below: 0.6em, it)
}
#show heading.where(level: 3): it => {
  set text(fill: highlight-blue, size: 9pt, weight: "bold")
  block(above: 1em, below: 0.5em, it)
}

// Code blocks - tight
#show raw.where(block: true): it => {
  set text(font: "Menlo", size: 7pt)
  block(
    fill: rgb("#f5f5f5"),
    inset: 6pt,
    radius: 2pt,
    width: 100%,
    it
  )
}

// Inline code
#show raw.where(block: false): it => {
  box(
    fill: rgb("#f0f0f0"),
    inset: (x: 3pt, y: 0pt),
    radius: 2pt,
    text(font: "Menlo", size: 0.9em, it)
  )
}

// Links
#show link: it => text(fill: accent-blue, it)

// Tables - tight
#set table(
  stroke: 0.5pt + rgb("#ddd"),
  inset: 4pt,
)
#show table.cell.where(y: 0): set text(weight: "bold")

// Pandoc compatibility functions
#let horizontalrule = line(length: 100%, stroke: 0.5pt + rgb("#ccc"))

// Dummy cite function - pandoc converts @references to cite() calls
// This renders them as plain text since we don't have a bibliography
#let cite(label, ..args) = {
  let key = if type(label) == "label" {
    str(label).trim("<>")
  } else {
    str(label)
  }
  text(fill: accent-blue, "@" + key)
}

// Table of Contents (depth 4 to include ADR titles under Appendix C)
#outline(
  title: text(fill: primary-blue, size: 18pt, weight: "bold", "Table of Contents"),
  indent: 1.5em,
  depth: 4,
)
#pagebreak()
