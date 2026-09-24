/**
 * What reading the vault into moments looks like from the front end.
 *
 * The shapes the Rust side sends (`commands::timeline`), and the one piece of
 * behaviour two screens share: the kinds a moment can be. They live in the
 * vault, so both the review — where a missing kind is noticed — and the
 * settings — where the list is kept — have to be able to add one.
 */
import { invoke } from '@tauri-apps/api/core';

export interface PersonRef {
    id: string;
    title: string;
}

export interface ExtractConfig {
    enabled: boolean;
    allow_cloud: boolean;
    folders: string[];
    tags: string[];
    conversations: boolean;
    categories: string[];
}

export interface Proposal {
    id: string;
    node_id: string;
    node_title: string;
    node_type: string;
    recorded: string;
    happened_from: string;
    happened_to: string;
    precision: string;
    title: string;
    people: PersonRef[];
    names: string[];
    quote: string;
    confidence: number;
    model: string;
    stale: boolean;
    category: string | null;
    amount: { value: number; unit: string } | null;
    about: string[];
    time: string | null;
    place: string | null;
    /** `explicit`, `relative`, `the_day` or `inferred`: how the day was worked out. */
    date_basis: string | null;
    /** The kept moment this would change, and how: `changed`, `retracted`, `gone` (§15). */
    about_moment: string | null;
    verdict: string | null;
}

/** A moment as it is kept, for holding a change up against it (§15). */
export interface MomentView {
    path: string;
    title: string;
    happened: string;
    people: string[];
    place: string | null;
    category: string | null;
    time: string | null;
    amount: { value: number; unit: string } | null;
    /** Fields the person wrote themselves. A change never touches these. */
    hand: string[];
}

/**
 * How reading is set up, and what waits for a decision.
 *
 * Answered from tables the timeline already holds: tens of milliseconds. How
 * much is left to read is `ReadingLeft`, and it is a different question with a
 * different price.
 */
export interface ExtractStatus {
    config: ExtractConfig;
    syn_enabled: boolean;
    provider: string;
    local: boolean;
    model: string | null;
    desktop: boolean;
    running: boolean;
    unreadable: string[];
    proposals: Proposal[];
    people: PersonRef[];
    /** The kinds a moment can be here: the vault's list, or the defaults. */
    categories: string[];
    moments: Record<string, MomentView>;
}

/**
 * How much of the vault has not been read, and what reading it would cost.
 *
 * Counted a day at a time from two queries, without opening a note — tens of
 * milliseconds whatever the vault holds. `null` means "not back yet" rather
 * than "none". What the day-at-a-time count gives up is written down on the
 * Rust side, on `reader::Left`.
 */
export interface ReadingLeft {
    /** Days with writing on them that no reading at this version has covered. */
    pending: number;
    /** Days covered only by an older reader. */
    old_version: number;
    /** Blocks this reader has already read. */
    done: number;
    estimate_ms: number;
    estimate_measured: boolean;
}

export interface ExtractRun {
    read: number;
    items: number;
    dropped: number;
    failed: string[];
    remaining: number;
    skipped: string | null;
}

/** What starting the timeline again would take away, and what it took. */
export interface ResetPlan {
    moments: number;
    proposals: number;
    readings: number;
    decisions: number;
    month_files: number;
    surrogates: number;
}

export interface ResetDone {
    moments: number;
    month_files: number;
    review_files: number;
    surrogates_kept: number;
    failed: string[];
}

/**
 * The list with one more kind in it, tidied the way the vault keeps it:
 * trimmed, lowercased, no duplicates, and `other` last — it is where what
 * fits nowhere goes.
 */
export function withKind(kinds: string[], added: string): string[] {
    const kind = added.trim().toLowerCase();
    if (!kind || kinds.includes(kind)) return kinds;
    return [...kinds.filter(k => k !== 'other'), kind, 'other'];
}

/** Keep the vault's list of kinds. The rest of the configuration is untouched. */
export async function saveKinds(vaultPath: string, config: ExtractConfig, categories: string[]): Promise<void> {
    await invoke('timeline_extract_configure', { vaultPath, settings: { ...config, categories } });
}
