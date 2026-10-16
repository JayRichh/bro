// WHAKAOHO (wake-up): the shapes `kete wake status --json` and `kete wake
// <account> --force --json` print, as the Rust side hands them over
// (`wake_overview`, `wake_now`). Field names follow the CLI's JSON
// (snake_case), which is the one source both apps read.

export type WakeMode = 'interactive' | 'print';
/** `auto` runs interactive and falls back to print while the daemon can't. */
export type WakeModeChoice = 'auto' | WakeMode;

/** One account's schedule, as `config.local.json` keeps it. */
export interface WakeSchedule {
  enabled: boolean;
  /** Minutes after local midnight, in 15-minute steps. */
  anchor_min: number;
  /** A bit per day, Monday first (127: every day). */
  days: number;
  keep_rolling: boolean;
  skip_idle_days?: number | null;
  /** Absent: auto. */
  mode?: WakeMode | null;
  wake_pc: boolean;
}

/** A five-hour window, minutes after midnight; `end` past 1440 runs into tomorrow. */
export interface WakeWindow {
  start: number;
  end: number;
}

export interface WakePreview {
  windows: WakeWindow[];
  /** Windows reaching into the usual hours; null without usual hours. */
  fresh_in_active: number | null;
  /** The same with no wake-up at all. */
  baseline: number | null;
}

/** The usual hours: a stretch of the local day, which may run past midnight. */
export interface UsualHours {
  start: number;
  len: number;
}

export interface WakeLogEntry {
  at: number;
  account: string;
  decision: 'fired' | 'skipped' | 'failed';
  reason: string;
  mode: WakeMode;
  resets_before: number | null;
  resets_after: number | null;
  detail?: string | null;
  slot?: number | null;
  forced?: boolean;
}

/** A plan window from the account's kept reading (`usage::UsageWindow`). */
export interface WakeSessionWindow {
  kind: string;
  label: string | null;
  percent: number;
  resets_at: number | null;
  read_at: number;
}

export interface WakeAccount {
  /** The account key: a profile's name, or `@claude`. */
  account: string;
  /** A Claude account this machine has (false: a schedule left for one that is gone). */
  known: boolean;
  disabled: boolean;
  logged_in: boolean;
  login_hint: string | null;
  schedule: WakeSchedule;
  /** `auto`, `interactive` or `print`. */
  mode: WakeModeChoice;
  /** `every day`, `Mon-Fri`, `Mon, Wed`. */
  days_text: string;
  /** The minutes of the day it fires at (the anchor, then keep-rolling steps). */
  slots: number[];
  /** Epoch seconds of the next wake-up; null when off. */
  next: number | null;
  usual: UsualHours | null;
  /** The anchor that fits the usual hours best; null without them. */
  suggested: number | null;
  preview: WakePreview;
  /** Every anchor's preview, by anchor / 15; null without usual hours. */
  previews: WakePreview[] | null;
  /** 96 quarter-hour turn counts over the last two weeks; null without a daemon. */
  bins: number[] | null;
  last_turn_at: number | null;
  last: WakeLogEntry | null;
  last_summary: string | null;
  recent: { entry: WakeLogEntry; summary: string }[];
  session: WakeSessionWindow | null;
}

export interface WakeOverview {
  /** Epoch seconds. */
  now: number;
  /** The local clock's offset from UTC, seconds east. */
  offset: number;
  /** null: no daemon running; false: one too old to run interactive wake-ups. */
  daemon: boolean | null;
  windows: boolean;
  task_name: string;
  /** What the trigger does now, in a line. */
  task_line: string;
  /** The task's times, `HH:MM`. */
  task: string[];
  /** macOS / Linux: the crontab lines to add by hand. */
  crontab: string[];
  log: string;
  wake_dir: string;
  /** The model a heartbeat runs on (`haiku`). */
  model: string;
  accounts: WakeAccount[];
}

/** What `kete wake <account> --force --json` printed for the account. */
export interface WakeNowResult {
  account: string;
  decision: 'fire' | 'skip';
  reason: string;
  detail: string | null;
  mode: WakeMode;
  report: {
    resets_before: number | null;
    resets_after: number | null;
    error: string | null;
    reason: string;
    submitted: boolean;
    finished: boolean;
    read: boolean;
  } | null;
}
