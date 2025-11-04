# DIY Patent Filing Guide - Step-by-Step

**For**: Filing provisional patents yourself via USPTO EFS-Web
**Time Required**: 2-3 hours
**Cost**: $260 total ($130 per provisional)
**Difficulty**: Medium (follow these instructions carefully)

---

## Prerequisites

Before you start, gather:

- [ ] Two provisional patent documents (convert to PDF):
  - `provisional-1-capability-composition.md` → `provisional-1.pdf`
  - `provisional-2-human-authority-control.md` → `provisional-2.pdf`
- [ ] Inventor information (name, address, citizenship)
- [ ] Company information (name, address)
- [ ] Credit card for payment ($260 total)
- [ ] 2-3 hours of uninterrupted time

**Tool for PDF conversion**:
- Mac: Open in Preview, Export as PDF
- Command line: `pandoc provisional-1-capability-composition.md -o provisional-1.pdf`
- Online: https://www.markdowntopdf.com/

---

## Step 1: Create USPTO Account (15 minutes)

### 1.1 Go to USPTO EFS-Web
- URL: https://www.uspto.gov/patents/apply/efs-web-patent
- Click **"EFS-Web Registered eFilers"**

### 1.2 Register New Account
- Click **"First Time Filer? Register Here"**
- Fill out form:
  - Email address (use company email)
  - Create username and password
  - Security questions
- **IMPORTANT**: Save username/password in password manager

### 1.3 Verify Email
- Check email for verification link
- Click link to activate account
- Log in to confirm account works

**Checkpoint**: You should see USPTO EFS-Web dashboard

---

## Step 2: Prepare Documents (45 minutes)

### 2.1 Add Prior Art Disclosure (if applicable)

**IMPORTANT**: If you have prior work connections (e.g., COD at Ditto), add prior art disclosure.

**Instructions**:
1. Open `provisional-1-capability-composition.md`
2. Find the section "## SUMMARY OF THE INVENTION"
3. Insert "## RELATED WORK AND PRIOR ART" section BEFORE summary
4. Copy content from `PRIOR_ART_ADDENDUM.md`
5. Customize for your specific situation
6. Repeat for `provisional-2-human-authority-control.md`

**Why this matters**:
- Shows good faith to USPTO
- Reduces risk of challenges from former employers
- Clearly distinguishes your novel innovations from prior work
- Strengthens patent by acknowledging prior art

**If no prior work connections**: Skip this step.

### 2.2 Convert to PDF

**Provisional #1: Capability Composition**
```bash
cd /Users/kit/Code/cap/docs/patents
pandoc provisional-1-capability-composition.md -o provisional-1.pdf \
  --pdf-engine=xelatex \
  --variable geometry:margin=1in
```

**Provisional #2: Human Authority Control**
```bash
pandoc provisional-2-human-authority-control.md -o provisional-2.pdf \
  --pdf-engine=xelatex \
  --variable geometry:margin=1in
```

### 2.2 Verify PDFs

Open each PDF and check:
- [ ] All text is readable
- [ ] Figures/code blocks are formatted correctly
- [ ] No missing sections
- [ ] Page numbers present

**File sizes**: Should be under 10 MB each (USPTO limit is 25 MB)

### 2.3 Prepare Cover Page Information

You'll need to type this during filing:

**Application Type**: Provisional Application for Patent

**Title of Invention #1**:
```
Hierarchical Capability Composition for Distributed Autonomous Systems
```

**Title of Invention #2**:
```
Graduated Human Authority Control for Distributed Autonomous Coordination Systems
```

**Inventors** (list all):
```
Name: Kit Plummer
Residence: [City, State, Country]
Citizenship: [Country]
```

**Applicant** (company):
```
Name: (r)evolve LLC
Address: [Full address with ZIP]
```

---

## Step 3: File Provisional #1 (45 minutes)

### 3.1 Log In to EFS-Web
- Go to: https://www.uspto.gov/patents/apply/efs-web-patent
- Click **"EFS-Web Registered eFilers"**
- Log in with your account

### 3.2 Start New Application
- Click **"File a New Application"**
- Select **"Provisional Application for Patent"**
- Click **"Continue"**

### 3.3 Application Data

**Correspondence Address**:
- Enter company address (where USPTO will send mail)
- Phone number
- Email address

**Application Information**:
- **Title of Invention**: `Hierarchical Capability Composition for Distributed Autonomous Systems`
- **Docket Number** (optional): `CAP-PROV-001`
- **Attorney Docket Number** (optional): Leave blank (or use `CAP-001`)

**Inventorship**:
- Click **"Add Inventor"**
- Enter inventor information:
  - Name: Kit Plummer
  - Residence: [City, State, Country]
  - Citizenship: [Country]
  - Mailing Address: [Address]
- If multiple inventors, click **"Add Inventor"** again

**Assignee** (company):
- Click **"Add Assignee"**
- Assignee Type: **"Company or Organization"**
- Name: `(r)evolve LLC`
- Address: [Company address]

### 3.4 Upload Documents

**Specification**:
- Click **"Add Document"**
- Document Description: **"Specification"**
- Click **"Choose File"** → Upload `provisional-1.pdf`
- Click **"Upload"**

**No other documents required** for provisional (no claims, drawings, or abstract required)

### 3.5 Review Application

- Click **"Review"**
- Carefully check all information:
  - Title correct?
  - Inventors listed?
  - Specification uploaded?
  - Address correct?

**Common mistakes**:
- ❌ Misspelled inventor names
- ❌ Wrong title
- ❌ Missing citizenship information
- ❌ Wrong correspondence address

### 3.6 Calculate Fees

- Click **"Calculate Fees"**
- **Entity Size**: Select **"Small Entity"** (most startups)
  - Small Entity: $130
  - Micro Entity: $65 (if you qualify - see USPTO criteria)
- **Total Fee**: $130 (or $65 if micro entity)

**Micro Entity Criteria** (if all are true, select Micro Entity):
- Fewer than 5 previous patent applications filed
- Individual gross income < $200K (or company < $1M revenue)
- Not obligated to assign to large entity

### 3.7 Submit and Pay

- Click **"Submit"**
- Enter credit card information
- Click **"Pay and Submit"**

**⏱️ Wait for confirmation** (30 seconds - 2 minutes)

### 3.8 Save Receipt

- Download **"Filing Receipt"** PDF
- Save as: `/secure/patents/provisional-1-filing-receipt.pdf`
- **IMPORTANT**: Note the **Application Number** (format: 63/XXX,XXX)
- **IMPORTANT**: Note the **Filing Date** (this is your priority date)

**Checkpoint**: You should have email confirmation from USPTO

---

## Step 4: File Provisional #2 (45 minutes)

Repeat Step 3 for the second provisional:

**Differences**:
- **Title**: `Graduated Human Authority Control for Distributed Autonomous Coordination Systems`
- **Docket Number**: `CAP-PROV-002`
- **Specification**: Upload `provisional-2.pdf`

**Everything else is the same** (same inventors, assignee, correspondence address)

---

## Step 5: Confirm Filing (10 minutes)

### 5.1 Check Email

Within 1-2 hours, you should receive:
- Filing receipt email from `ebc@uspto.gov`
- Application number confirmation
- Fee payment confirmation

### 5.2 Verify in EFS-Web

- Log in to EFS-Web
- Click **"Check Filing Status"**
- You should see both applications:
  - Application 63/XXX,XXX (Provisional #1)
  - Application 63/YYY,YYY (Provisional #2)
  - Status: "New - Not Yet Docketed"

### 5.3 Save All Documents

Create secure folder structure:
```
/secure/patents/
  ├── provisional-1-filing-receipt.pdf
  ├── provisional-2-filing-receipt.pdf
  ├── provisional-1.pdf (copy of what you filed)
  ├── provisional-2.pdf (copy of what you filed)
  ├── filing-confirmation-email-1.pdf
  ├── filing-confirmation-email-2.pdf
  └── uspto-account-credentials.txt (encrypted!)
```

**Backup these files** to cloud storage (Dropbox, Google Drive, etc.)

---

## Step 6: Update Documentation (30 minutes)

Now update your CAP Protocol docs with filing information:

### 6.1 Update PATENT_STRATEGY.md

```bash
cd /Users/kit/Code/cap/docs
nano PATENT_STRATEGY.md
```

Add at bottom:
```markdown
## Filing Record

| Patent | Application Number | Filing Date | Status |
|--------|-------------------|-------------|--------|
| Capability Composition | 63/XXX,XXX | 2025-11-XX | Provisional Filed |
| Human Authority Control | 63/YYY,YYY | 2025-11-XX | Provisional Filed |

## Important Dates

- **Priority Date**: 2025-11-XX
- **Utility Conversion Deadline**: 2026-11-XX (12 months from filing)
- **Month 12 Decision Meeting**: 2026-10-XX (schedule in advance)
```

### 6.2 Update PATENT_PLEDGE.md

```bash
nano PATENT_PLEDGE.md
```

Update lines:
```markdown
**Effective Date**: 2025-11-XX

### Filed Patents
- **US Provisional Application 63/XXX,XXX**: "Hierarchical Capability Composition..." (filed 2025-11-XX)
- **US Provisional Application 63/YYY,YYY**: "Graduated Human Authority Control..." (filed 2025-11-XX)
```

And at bottom:
```markdown
**Signed**:
Kit Plummer, [Title]
(r)evolve LLC
November XX, 2025
```

### 6.3 Commit and Push

```bash
git add docs/PATENT_STRATEGY.md docs/PATENT_PLEDGE.md
git commit -m "docs: Update patent filing information with USPTO application numbers"
git push
```

---

## Step 7: Set Calendar Reminders (10 minutes)

Add these to your calendar:

### Reminder 1: Month 6 Check-In
```
Event: Patent Strategy Mid-Year Check-In
Date: [6 months from filing]
Reminder: 1 week before
Notes:
  - Review competitive patent filings
  - Gather customer feedback
  - Update evidence log
```

### Reminder 2: Month 11 Decision Meeting
```
Event: Patent Strategy Decision Meeting
Date: [11 months from filing]
Reminder: 1 month before
Attendees: Kit, legal counsel, technical lead
Agenda:
  1. Review evidence from past year
  2. Decide: Convert to utility or abandon
  3. If convert: Hire patent attorney
  4. If abandon: Publish disclosures
Notes:
  - CRITICAL: Must decide within 12 months of filing
  - Deadline: [12 months from filing date]
```

### Reminder 3: ABSOLUTE DEADLINE
```
Event: ⚠️ PATENT UTILITY CONVERSION DEADLINE ⚠️
Date: [12 months from filing]
Reminder: 2 weeks before, 1 week before, 1 day before
Notes:
  - If converting: File utility patent applications
  - If abandoning: Do nothing (provisionals expire)
  - CANNOT MISS THIS DEADLINE - lose all patent rights
```

---

## Troubleshooting

### Problem: PDF upload fails
**Solution**:
- Check file size (must be < 25 MB)
- Try converting to PDF again with different tool
- Ensure PDF is not password-protected

### Problem: Credit card declined
**Solution**:
- Check card has $260 available
- Try different card
- Use USPTO deposit account (requires setup)

### Problem: Can't find application after filing
**Solution**:
- Wait 24-48 hours for USPTO system to update
- Check spam folder for confirmation email
- Log in to EFS-Web → "Check Filing Status"

### Problem: Made mistake in filing
**Solution**:
- For provisionals: File corrected version immediately (explain in cover letter)
- Contact USPTO at 800-786-9199
- Provisionals are forgiving - can correct in utility application

### Problem: Uncertain about entity size
**Solution**:
- When in doubt, select "Small Entity" ($130)
- Can always downgrade to Micro Entity later (but not upgrade without penalty)
- See: https://www.uspto.gov/patents/basics/types-patent-applications/provisional-application-patent

---

## What Happens Next?

### Immediate (Within 48 Hours)
- USPTO sends filing receipt email
- Application assigned number (63/XXX,XXX)
- Status: "New - Not Yet Docketed"

### Week 1-2
- Follow POST_FILING_CHECKLIST.md
- Publish Patent Pledge
- Open-source CAP Protocol

### Month 1-11
- Track evidence for utility decision
- Monitor competitive patents
- Collect customer feedback

### Month 12 (CRITICAL)
- Decide: Convert to utility or abandon
- If converting: Hire patent attorney, file utility applications
- If abandoning: Provisionals expire, become defensive prior art

---

## Cost Summary

| Item | Cost | When |
|------|------|------|
| Provisional #1 filing fee | $130 | Today |
| Provisional #2 filing fee | $130 | Today |
| **Total Today** | **$260** | **Paid now** |
| | | |
| Optional: Legal review | $300-$500 | Next week |
| Optional: Utility conversion | $20K-$40K | Month 12 |

---

## Success Criteria

You're done when:

- [x] Both provisionals filed with USPTO
- [x] Received filing receipts with application numbers
- [x] Documentation updated with filing info
- [x] Calendar reminders set for Month 6, 11, 12
- [x] Files backed up securely
- [x] Ready to publish Patent Pledge

**Next steps**: See POST_FILING_CHECKLIST.md

---

## Resources

**USPTO Help**:
- Phone: 800-786-9199 (Mon-Fri, 8:30am-5pm ET)
- Email: ebc@uspto.gov
- Live chat: Available on USPTO.gov

**EFS-Web**:
- URL: https://www.uspto.gov/patents/apply/efs-web-patent
- User Guide: https://www.uspto.gov/patents/apply/efs-web-user-guide

**Fees**:
- Fee Schedule: https://www.uspto.gov/learning-and-resources/fees-and-payment/uspto-fee-schedule
- Entity Size: https://www.uspto.gov/patents/basics/types-patent-applications/provisional-application-patent#fees

---

**Status**: Ready to File
**Estimated Time**: 2-3 hours total
**Difficulty**: Medium (follow instructions carefully)
**Cost**: $260 (small entity) or $130 (micro entity)

**Good luck!** 🚀
