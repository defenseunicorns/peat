# Peat Protocol - Patent Filing Package

This directory contains technical disclosure documents for provisional patent applications related to Peat Protocol innovations.

## Documents Overview

### 📋 Quick Start (Read These First)

1. **EXECUTIVE_SUMMARY.md** - 1-page TL;DR for leadership decision
2. **DIY_FILING_GUIDE.md** - Step-by-step filing instructions (2-3 hours)
3. **POST_FILING_CHECKLIST.md** - What to do immediately after filing

### 📄 Core Documents

4. **PATENT_STRATEGY.md** (in parent directory) - Comprehensive strategy analysis (28 pages)
5. **PATENT_PLEDGE.md** (in parent directory) - Defensive use commitment (10 pages)

### 📝 Patent Applications (Ready to File)

6. **provisional-1-capability-composition.md** - Technical disclosure #1 (45 pages)
   - **Title**: "Hierarchical Capability Composition for Distributed Autonomous Systems"
   - **Innovation**: Additive, emergent, redundant, and constraint-based composition using CRDTs
   - **Key Claims**: O(n log n) message complexity, emergent capabilities, partition tolerance

7. **provisional-2-human-authority-control.md** - Technical disclosure #2 (42 pages)
   - **Title**: "Graduated Human Authority Control for Distributed Autonomous Coordination Systems"
   - **Innovation**: Five-level authority taxonomy (FULL_AUTO → MANUAL) with distributed enforcement
   - **Key Claims**: Approval/veto protocols, cryptographic audit trail, hierarchical constraints

### 📚 Reference Documents

8. **README.md** (this file) - Index and filing instructions
9. **POST_FILING_CHECKLIST.md** - 48-hour action plan after filing
10. **DIY_FILING_GUIDE.md** - Detailed USPTO filing walkthrough

---

## Quick Navigation

**If you want to...**

- **Understand the strategy** → Read `EXECUTIVE_SUMMARY.md`
- **File patents yourself** → Follow `DIY_FILING_GUIDE.md`
- **Review technical details** → Read `provisional-1-*.md` and `provisional-2-*.md`
- **Understand cost/timeline** → See tables below or `PATENT_STRATEGY.md`
- **Know what happens after filing** → Follow `POST_FILING_CHECKLIST.md`
- **Address NATO/academic concerns** → Share `PATENT_PLEDGE.md`

## Filing Checklist

### Pre-Filing (This Week)

- [ ] Review both technical disclosures for accuracy
- [ ] Decide filing approach:
  - [ ] **DIY Filing** ($260 total) - Fastest, establishes priority
  - [ ] **Budget Attorney** ($4K-$6K) - Better claims, professional review
  - [ ] **Full-Service Firm** ($10K+) - Maximum protection
- [ ] Confirm inventor names and contact information
- [ ] Prepare USPTO.gov account (if DIY filing)

### Filing Process

#### Option A: DIY Filing via USPTO EFS-Web

1. **Create USPTO Account**
   - Go to: https://www.uspto.gov/patents/apply/efs-web-patent
   - Create account or log in

2. **Start New Application**
   - Application Type: Provisional Patent Application
   - Entity Size: Small Entity (most startups) or Micro Entity

3. **Upload Documents**
   - Specification: Upload `provisional-1-capability-composition.md` (convert to PDF)
   - Specification: Upload `provisional-2-human-authority-control.md` (convert to PDF)
   - Note: Can file as separate applications or combined

4. **Complete Inventor Information**
   - Name: Kit Plummer (and any co-inventors)
   - Address, citizenship

5. **Pay Filing Fee**
   - Small Entity: $130 per application = $260 total
   - Micro Entity: $65 per application = $130 total
   - Credit card or USPTO deposit account

6. **Submit and Save Receipt**
   - File electronically
   - Download filing receipt with priority date
   - **This is your proof of filing date - SAVE IT**

#### Option B: Hire Patent Attorney

1. **Find USPTO-Registered Attorney**
   - Search: https://oedci.uspto.gov/OEDCI/
   - Look for "Patent Law" and defense/aerospace experience
   - Budget firms: $2K-$3K per provisional
   - Full-service: $5K+ per provisional

2. **Provide Technical Disclosures**
   - Share both `.md` files
   - Schedule inventor interview (1-2 hours)
   - Attorney will refine claims and specification

3. **Review and Approve**
   - Attorney sends draft for review
   - Provide feedback, iterate

4. **Attorney Files**
   - Attorney handles USPTO filing
   - You pay filing fees + attorney fees

### Post-Filing (Immediate)

- [ ] Receive filing receipt with priority date
- [ ] Store receipt securely (digital + printed backup)
- [ ] Update `PATENT_STRATEGY.md` with filing dates and application numbers
- [ ] Update `../PATENT_PLEDGE.md` with application numbers and effective date
- [ ] Publish Patent Pledge on website and GitHub
- [ ] Open-source Peat Protocol (now protected by provisional + pledge)
- [ ] Add "Patent Pending" + link to pledge in README files

### Timeline

| Event | Date | Action |
|-------|------|--------|
| **Week 1** | This week | File provisionals (DIY or attorney) |
| **Week 2** | After filing | Open-source Peat Protocol |
| **Month 1-12** | Throughout year | Collect evidence for utility decision |
| **Month 11** | Oct 2025 | Schedule decision meeting |
| **Month 12** | Nov 2025 | Decide: Convert to utility or abandon |

### Month 12 Decision Criteria

**Convert to Utility Patents ($20K-$40K) if:**
- Government customer wants exclusive license or patent protection
- Competitor files similar patents
- Investors require patents for Series A valuation
- Strong interest in commercial licensing

**Abandon Provisionals ($0 cost) if:**
- No competitive threats
- Customers happy with GOTS approach
- Strong open-source momentum
- $40K not worth strategic value

## Cost Summary

### Immediate Costs (Week 1)

| Option | Cost | Timeline | Quality |
|--------|------|----------|---------|
| DIY Filing | $260 | 1 week | Good enough |
| Budget Attorney | $4K-$6K | 2-4 weeks | Better |
| Full-Service | $10K+ | 4-6 weeks | Best |

### Future Costs (Month 12, if converting)

| Item | Cost | Timeline |
|------|------|----------|
| Utility Patent Filing | $10K-$20K per patent | Month 12 |
| Patent Prosecution | $5K-$15K per patent | Years 1-3 |
| Maintenance Fees | $400-$7,400 | Years 4, 8, 12 |
| **Total per Patent** | **$15K-$42K** | **Over patent lifetime** |

## Filing Details

### Provisional Patent #1

**Title**: Hierarchical Capability Composition for Distributed Autonomous Systems

**Inventors**: Kit Plummer, et al.

**Abstract** (for USPTO form):
> A system and method for hierarchical capability composition in distributed autonomous systems using Conflict-free Replicated Data Types (CRDTs). Autonomous platforms organize into hierarchical cells (squads, platoons, companies). Each cell's capabilities are automatically computed from member capabilities using composition rules: additive (union), emergent (new capabilities from specific combinations), redundant (threshold requirements), and constraint-based (dependencies, exclusions). Hierarchical aggregation achieves O(n log n) message complexity vs O(n²) for flat architectures. System is partition-tolerant: cells continue operating during network partitions and reconcile when reconnected.

**Keywords**: Autonomous systems, CRDT, distributed computing, capability composition, hierarchical networks, UAV coordination

### Provisional Patent #2

**Title**: Graduated Human Authority Control for Distributed Autonomous Coordination Systems

**Inventors**: Kit Plummer, et al.

**Abstract** (for USPTO form):
> A system and method for graduated human authority control in distributed autonomous coordination systems using Conflict-free Replicated Data Types (CRDTs). Five authority levels provide graduated autonomy: FULL_AUTO (no approval), SUPERVISED (human notified), HUMAN_APPROVAL (explicit approval required), HUMAN_VETO (human can block), and MANUAL (direct control). System handles human unavailability through configurable timeout policies. All autonomous decisions and human interventions are logged with cryptographic signatures in immutable audit trail. System is partition-tolerant: operates during network disruptions and reconciles state when connectivity restored.

**Keywords**: Autonomous systems, human-machine teaming, authority control, CRDT, distributed computing, weapons systems, DoD compliance

## USPTO Resources

### Filing Resources
- **EFS-Web (Online Filing)**: https://www.uspto.gov/patents/apply/efs-web-patent
- **Provisional Application Guide**: https://www.uspto.gov/patents/basics/types-patent-applications/provisional-application-patent
- **Fee Schedule**: https://www.uspto.gov/learning-and-resources/fees-and-payment/uspto-fee-schedule

### Attorney Search
- **USPTO Registered Attorney Search**: https://oedci.uspto.gov/OEDCI/
- **National Law Review Directory**: https://www.natlawreview.com/

### Prior Art Search (Before Filing)
- **Google Patents**: https://patents.google.com/
  - Search: "capability composition autonomous"
  - Search: "human authority control autonomous"
- **USPTO Public Search**: https://ppubs.uspto.gov/pubwebapp/
- **Academic Literature**: IEEE Xplore, ACM Digital Library

## Confidentiality

**IMPORTANT**: These documents contain confidential business information and trade secrets. Do NOT publicly disclose until:
1. Provisional patents are filed with USPTO (priority date established), OR
2. You decide not to pursue patent protection

After filing provisionals, you can:
- ✅ Open-source code implementations
- ✅ Publish technical papers
- ✅ Present at conferences
- ✅ Share with customers/partners

## Next Steps

1. **Review technical disclosures** (both provisional documents)
2. **Decide filing approach** (DIY vs attorney)
3. **File provisionals** (establishes priority date)
4. **Save filing receipts** (proof of priority date)
5. **Open-source Peat Protocol** (now protected)
6. **Update PATENT_STRATEGY.md** with filing details

## Questions?

**Technical questions**: Review `PATENT_STRATEGY.md`
**Filing questions**: USPTO helpline (800-786-9199) or consult patent attorney
**Strategic questions**: Discuss with Defense Unicorns leadership

## Document History

| Date | Version | Changes |
|------|---------|---------|
| 2025-11-04 | 1.0 | Initial drafts of both provisionals |
| TBD | 1.1 | Post-filing update with app numbers |
| TBD | 2.0 | Month 12 decision on utility conversion |

---

**Status**: Ready for filing
**Priority**: High (file this week to establish priority date before any public disclosure)
**Owner**: Kit Plummer / Defense Unicorns LLC
