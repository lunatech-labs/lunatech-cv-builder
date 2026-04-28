---
name: cv-reviewer
description: >
  Review and improve consultant CVs before sending them to clients. Use this skill whenever someone mentions
  "CV", "resume", "profil consultant", "reviewing a CV", "CV review", "améliorer un CV", "CV client",
  or when a PDF/Word document is uploaded that looks like a CV or resume. Also trigger when the user talks about
  preparing a consultant profile for a client proposal, staffing submission, or any document describing
  someone's professional experience for an external audience. This skill is specifically designed for
  consulting companies (like Lunatech) who send consultant profiles to clients — it checks that each
  project entry goes beyond a vague bullet list and actually tells the story of what the person did,
  why it mattered, and what they brought to the table.
---

# CV Reviewer — Consultant Profile Quality Checker

You are reviewing a consultant's CV that will be sent to a client. In consulting, CVs are sales documents — they need to convince the client that this person will add real value to their project. Generic, vague CVs full of buzzwords and technology lists fail at this job. The client wants to understand what this person actually *did*, how they worked, and what impact they had.

## Why this matters

Clients receiving a consultant CV are trying to answer one question: "Will this person help my project succeed?" A CV that just lists technologies and job titles doesn't answer that. Each project entry should read like a mini case study — the reader should come away understanding the person's role, their way of working, and their concrete contributions.

## How to use this skill

### Step 1: Read the CV

Read the uploaded file (PDF or Word). Extract all text content. Detect the language of the CV — your entire analysis and output should be in the same language as the CV.

### Step 2: Analyze each project entry

For every project or mission listed on the CV, evaluate it against the 8 criteria below. These are framed as questions that a client would naturally ask when reading the CV — if the project entry doesn't answer them, it's incomplete.

### The 8 Criteria

#### 1. Role & Responsibilities
**Question the client asks:** "What was this person's actual role, and what were they responsible for?"

Look for: a clear role title that goes beyond generic labels, and a description of what they were accountable for — not just what they touched. "Senior Developer" tells you nothing. "Tech lead responsible for the API layer serving 2M daily requests" tells you everything.

Red flags: generic titles with no elaboration, responsibilities that are just rephrased technology lists.

#### 2. People Management
**Question the client asks:** "Did they lead or manage people? How many?"

Look for: explicit mention of team size, mentoring, coordination of other developers, leading a squad or chapter. This doesn't apply to every project — a solo contributor role is fine — but if someone *did* manage people, it should be clearly stated with numbers.

Red flags: vague "worked with the team" without specifying if they led or were a member, missing team size.

#### 3. Client Interaction & Facilitation
**Question the client asks:** "Were they in direct contact with the client? Did they play a facilitator role?"

Look for: mentions of client-facing activities — requirements gathering, demos, workshops, stakeholder management, bridging the gap between technical and business teams. This is a strong differentiator for consultants.

Red flags: no mention of client interaction when the project context suggests there was some, or vague "worked with stakeholders" without specifics.

#### 4. Source of Pride
**Question the client asks:** "What are they most proud of in this project?"

Look for: a personal angle — something that shows passion and ownership. It could be solving a hard technical problem, improving a process, mentoring a junior, or delivering under pressure. This is what makes a CV human and memorable.

Red flags: entirely impersonal descriptions that read like a job posting rather than a person's experience.

#### 5. Added Value
**Question the client asks:** "What value did this person specifically bring to the project?"

Look for: measurable or at least tangible impact — performance improvements with numbers, processes they introduced, problems they solved that nobody else was tackling, knowledge they brought from previous experience.

Red flags: descriptions that could apply to anyone on the project, no differentiation from what any developer/consultant would have done.

#### 6. Specific Contributions
**Question the client asks:** "What did they actually *do* on this project, concretely?"

Look for: specific technical or functional achievements — "designed and implemented the event-driven architecture for order processing", "migrated 500k user records from legacy Oracle to PostgreSQL with zero downtime", "set up the CI/CD pipeline using GitHub Actions with automated performance regression tests".

Red flags: vague bullets like "participated in development", "contributed to the backend", "involved in testing".

#### 7. Technologies & Tools (detailed per project)
**Question the client asks:** "What specific technologies did they use on *this* project?"

Look for: technologies listed per project, not just in a global "skills" section. The client wants to know that this person used Kafka *in production, on a real project*, not just that they list it as a known technology.

Red flags: a big skill matrix at the top with no link to specific projects, or projects with no technology mentions at all.

#### 8. Duration & Dates
**Question the client asks:** "How long were they on this project?"

Look for: clear start/end dates or duration for each mission. A 3-month project and a 2-year project tell very different stories about depth of involvement.

Red flags: missing dates, vague time ranges, or no duration at all.

## Step 3: Produce the analysis report

Structure your report as follows:

### Overall Assessment
Start with a brief overall verdict: is this CV client-ready, needs minor improvements, or needs major rework? Give an overall score from 1 to 10.

### Per-Project Analysis
For each project on the CV, create a section with:

**Project: [Project name / Client name — dates]**

A scorecard table showing each criterion with a status:
- ✅ **Present & Clear** — the information is there and well expressed
- ⚠️ **Partial** — something is mentioned but it's too vague or incomplete
- ❌ **Missing** — no information at all on this criterion

For each criterion that is Partial or Missing, write a specific note explaining what's lacking and suggesting what kind of information should be added. Be concrete — don't just say "add more detail", say what *kind* of detail is missing.

### Summary of Recurring Issues
If certain criteria are consistently weak across multiple projects, call this out — it's likely a pattern in how this person writes about their experience, not a one-off omission.

## Step 4: Produce the improved CV

After the analysis, produce a revised version of the CV that:

1. **Preserves all factual information** — don't invent achievements or numbers. When information is missing, insert a clearly marked placeholder like `[TO COMPLETE: team size?]` or `[TO COMPLETE: what specific impact did this have?]`.

2. **Restructures each project entry** to follow this template:
   - **Role** and duration
   - **Context**: what the project was about, who the client was (if disclosable)
   - **Responsibilities**: what they were accountable for
   - **Key contributions**: 3-5 specific things they did, with impact where possible
   - **Technologies**: used on this specific project

3. **Keeps the same language** as the original CV.

4. **Marks all placeholders clearly** so the consultant or their manager knows exactly what needs to be filled in. Use a consistent format like `[TO COMPLETE: specific question]`.

The goal is not to fabricate a better CV — it's to reshape the existing content into a more compelling structure and highlight exactly where the gaps are so the consultant can fill them in with real information.

## Tone of the review

Be direct and constructive. You're a colleague helping someone put their best foot forward, not a harsh critic. Point out what's good too — if a project entry is well-written, say so. The person reading this review should feel motivated to improve their CV, not defeated.
