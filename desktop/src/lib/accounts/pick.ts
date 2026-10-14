// The quick prompt's profile picker, the pure half (tested in
// pick.test.ts): an explicit "auto" that leaves the choice to the ACCOUNT
// SPREAD (`nebula_core::accounts`) and says what it would pick, then each
// profile by name. Sending no profile is what lets the spread run; a named
// one always runs under that name, an off one too.

import type { Agent, AutoPick, Launchables } from '../api';

export type LaunchProfile = Launchables['profiles'][number];

/** The auto option's value: no profile sent, the spread (or default) decides. */
export const AUTO = '';

const SPREAD_WORD: Record<string, string> = { manual: 'default', failover: 'failover', least_used: 'least used' };

/** Where the picker starts on a harness: auto while a spread is on (so it
 *  can choose), under `manual` the default in force, else auto (which then
 *  is the CLI's own login). */
export function initialProfile(profiles: LaunchProfile[], spread: string | undefined): string {
  if (spread && spread !== 'manual') return AUTO;
  return profiles.find((p) => p.default && !p.disabled)?.name ?? AUTO;
}

/** Is `current` still a choice on this list (auto always is)? */
export function stillOffered(current: string, profiles: LaunchProfile[]): boolean {
  return current === AUTO || profiles.some((p) => p.name === current);
}

/** The auto option's words: `auto (failover \u2192 home)`, `auto (default \u2192
 *  work)`, `auto (own login)`; with no profiles, just the CLI's own login. */
export function autoLabel(profiles: LaunchProfile[], spread: string | undefined, pick: AutoPick | null): string {
  if (!profiles.length) return 'own login';
  const mode = SPREAD_WORD[pick?.spread ?? spread ?? 'manual'] ?? spread ?? 'default';
  if (!pick) return `auto (${mode})`;
  if (!pick.profile) return `auto (${mode === 'default' ? '' : `${mode} \u2192 `}own login)`;
  return `auto (${mode} \u2192 ${pick.profile})`;
}

export interface ProfileOption {
  value: string;
  label: string;
  /** The longer why, for the select's tooltip. */
  title: string;
}

/** Every option, auto first: a profile marked `(default)` when a launch
 *  naming none takes it, `(off)` when switched off. */
export function profileOptions(profiles: LaunchProfile[], spread: string | undefined, pick: AutoPick | null): ProfileOption[] {
  const auto: ProfileOption = {
    value: AUTO,
    label: autoLabel(profiles, spread, pick),
    title: !profiles.length
      ? "No profiles: the session runs on the CLI's own login."
      : `Auto: no profile is named, so the account spread chooses${pick ? `. Now: ${pick.line}` : ''}.`,
  };
  return [
    auto,
    ...profiles.map((p) => ({
      value: p.name,
      label: `${p.name}${p.default ? ' (default)' : ''}${p.disabled ? ' (off)' : ''}`,
      title: p.disabled
        ? `${p.name} is off: auto never picks it, but this launch names it.`
        : `Runs under ${p.name}${p.default ? ', the default' : ''}.`,
    })),
  ];
}

/** The remove confirmation's words, true to what the daemon does: a
 *  session keeps its profile's name, and a boot under a name no longer
 *  defined is refused (`profiles::resolve`), not moved to another login. */
export function removeWarning(name: string, agents: Pick<Agent, 'profile' | 'archived'>[]): string {
  const under = agents.filter((a) => a.profile?.trim() === name);
  const live = under.filter((a) => !a.archived).length;
  const archived = under.length - live;
  const head = `Remove the profile "${name}"?`;
  if (!under.length) return `${head} No session runs under it.`;
  const count = [
    live ? `${live} ${live === 1 ? 'session runs' : 'sessions run'} under it` : '',
    archived ? `${archived} archived ${archived === 1 ? 'one names' : 'ones name'} it` : '',
  ]
    .filter(Boolean)
    .join(', ');
  return `${head} ${count}. A running session keeps going, but none of them can start again (resume, restart): kete refuses with "profile \`${name}\` is no longer defined" until a profile of that name is added back.`;
}
