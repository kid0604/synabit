/**
 * What a person's frontmatter looks like, as the People form writes it.
 *
 * # Why this is declared at all
 *
 * The form built its properties as `Record<string, any>`, so the shape lived
 * only in the code that wrote it. Syn, asked to add a colleague, wrote
 * `experiences` as a list of sentences and `relationship_type` as a string —
 * nothing it could read said otherwise — and the form dropped the job the
 * first time the person was saved.
 *
 * `PersonModal` builds its write against this, and the assistant's description
 * of a person (`app_fields` in `src-tauri/src/syn/tools.rs`) is checked against
 * it by a Rust test, so the two cannot drift apart without one of them failing.
 *
 * Other screens own other keys — interactions, gifts, `last_contacted`,
 * `connections`, `is_owner` — and they are deliberately not here: this is what
 * the form writes, not everything a person may carry.
 */

export interface PersonExperience {
  company: string;
  role: string;
  /** `YYYY-MM`, or `YYYY`, or empty. */
  start: string;
  /** Same shape as `start`; empty while `current`. */
  end: string;
  current: boolean;
}

export interface PersonDetail {
  label: string;
  value: string;
  type: 'text' | 'email' | 'phone' | 'url';
}

export interface PersonMetadata {
  avatar: string | null;
  nickname: string | null;
  display_name: 'fullname' | 'nickname' | 'custom';
  custom_display: string | null;
  /** A list. An old vault may hold one comma-separated string; readers accept both. */
  relationship_type: string[] | null;
  /** weekly, biweekly, monthly, quarterly or yearly; null when not tracked. */
  contact_frequency: string | null;
  /** `YYYY-MM-DD`. */
  birthday: string | null;
  tags: string[] | null;
  important_dates: Array<{ label: string; date: string }> | null;
  experiences: PersonExperience[] | null;
  details: PersonDetail[] | null;
  /** Shortcuts copied out of `details`, for search and the sidebar. */
  email: string | null;
  phone: string | null;
  company: string | null;
}
