#!/bin/bash
# HIVE Protocol Demo - GitHub Label Setup Script
# 
# This script creates all labels for the kitplummer/hive repository
# using the GitHub CLI (gh).
#
# Prerequisites:
#   - GitHub CLI installed: https://cli.github.com/
#   - Authenticated: gh auth login
#
# Usage:
#   chmod +x setup-labels.sh
#   ./setup-labels.sh
#
# Organization: (r)evolve - Revolve Team LLC
# https://revolveteam.com

set -e

REPO="kitplummer/hive"

echo "🏷️  Setting up GitHub labels for $REPO"
echo "================================================"

# Function to create or update a label
create_label() {
    local name="$1"
    local color="$2"
    local description="$3"
    
    echo "Creating label: $name"
    gh label create "$name" --color "$color" --description "$description" --repo "$REPO" 2>/dev/null || \
    gh label edit "$name" --color "$color" --description "$description" --repo "$REPO" 2>/dev/null || \
    echo "  ⚠️  Could not create/update: $name"
}

echo ""
echo "📦 Team Labels"
echo "------------------------------------------------"
create_label "team/core" "0052CC" "Core team: Schema, protocol, reference implementation"
create_label "team/atak" "5319E7" "ATAK team: Android plugin, CoT translation, TAK integration"
create_label "team/experiments" "006B75" "Experiments team: Scale labs, network simulation, validation"
create_label "team/ai" "D93F0B" "AI team: Jetson inference, MLOps, model management"
create_label "team/pm" "FBCA04" "Project management: Coordination, scheduling, stakeholders"

echo ""
echo "🎯 Vignette Phase Labels"
echo "------------------------------------------------"
create_label "phase/1-init" "C2E0C6" "Phase 1: Initialization & Capability Advertisement"
create_label "phase/2-tasking" "C2E0C6" "Phase 2: Mission Tasking via TAK"
create_label "phase/3-tracking" "C2E0C6" "Phase 3: Active Tracking & Track Updates"
create_label "phase/4-handoff" "C2E0C6" "Phase 4: Cross-Network Track Handoff"
create_label "phase/5-mlops" "C2E0C6" "Phase 5: MLOps Model Distribution & Hot-Swap"

echo ""
echo "📋 Type Labels"
echo "------------------------------------------------"
create_label "type/schema" "1D76DB" "Schema definition or change"
create_label "type/integration" "1D76DB" "Cross-team integration work"
create_label "type/blocker" "B60205" "Blocking another team's progress"
create_label "type/dependency" "FBCA04" "Has external or cross-team dependency"
create_label "type/contract" "5319E7" "Interface contract definition"
create_label "type/validation" "0E8A16" "Validation or acceptance testing"
create_label "type/documentation" "0075CA" "Documentation update required"
create_label "type/bug" "D73A4A" "Something isn't working"
create_label "type/enhancement" "A2EEEF" "New feature or request"

echo ""
echo "🚨 Priority Labels"
echo "------------------------------------------------"
create_label "priority/p0-blocker" "B60205" "Critical: Blocks demo, immediate attention required"
create_label "priority/p1-critical" "D93F0B" "High: Required for current sprint milestone"
create_label "priority/p2-normal" "FBCA04" "Normal: Standard priority work item"
create_label "priority/p3-low" "0E8A16" "Low: Nice to have, can defer"

echo ""
echo "📊 Status Labels"
echo "------------------------------------------------"
create_label "status/needs-triage" "D4C5F9" "Needs team assignment and priority"
create_label "status/blocked" "B60205" "Blocked by external dependency"
create_label "status/in-review" "FBCA04" "PR submitted, awaiting review"
create_label "status/integration-test" "0E8A16" "Ready for cross-team integration testing"
create_label "status/demo-ready" "0E8A16" "Validated and ready for demo"

echo ""
echo "🔧 Component Labels"
echo "------------------------------------------------"
create_label "component/schema" "BFD4F2" "JSON schema definitions"
create_label "component/automerge" "BFD4F2" "Automerge CRDT sync engine"
create_label "component/iroh" "BFD4F2" "Iroh networking layer"
create_label "component/tak-bridge" "BFD4F2" "HIVE-TAK Bridge / CoT translation"
create_label "component/atak-plugin" "BFD4F2" "ATAK Android plugin"
create_label "component/jetson" "BFD4F2" "Jetson edge compute / inference"
create_label "component/mlops" "BFD4F2" "Model distribution and lifecycle"
create_label "component/containerlab" "BFD4F2" "Network simulation topology"

echo ""
echo "================================================"
echo "✅ Label setup complete!"
echo ""
echo "Next steps:"
echo "  1. Copy issue templates to .github/ISSUE_TEMPLATE/"
echo "  2. Create GitHub Projects board"
echo "  3. Start creating Sprint 1 issues"
echo ""
