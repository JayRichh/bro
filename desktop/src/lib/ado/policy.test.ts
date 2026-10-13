import { describe, expect, it } from 'vitest';
import { preflight } from './policy';

const ana = { id: 'u1', name: 'Ana Synth', email: 'ana@synth.test' };

describe('preflight', () => {
  it('blocks without a task unless the escape is set', () => {
    const r = preflight({ task: null, noTask: false, reviewers: [ana], noReviewer: false });
    expect(r).toEqual({ ok: false, notes: ['Link a task (policy)'] });
  });

  it('turns the missing task into a note with the escape', () => {
    const r = preflight({ task: null, noTask: true, reviewers: [ana], noReviewer: false });
    expect(r).toEqual({ ok: true, notes: ['No task (n/a)'] });
  });

  it('blocks with zero reviewers unless the escape is set', () => {
    const r = preflight({ task: 4211, noTask: false, reviewers: [], noReviewer: false });
    expect(r).toEqual({ ok: false, notes: ['Add a reviewer (policy)'] });
  });

  it('turns zero reviewers into a note with the escape', () => {
    const r = preflight({ task: 4211, noTask: false, reviewers: [], noReviewer: true });
    expect(r).toEqual({ ok: true, notes: ['No reviewer (n/a)'] });
  });

  it('passes clean with a task and a reviewer', () => {
    expect(preflight({ task: 4211, noTask: false, reviewers: [ana], noReviewer: false })).toEqual({ ok: true, notes: [] });
  });

  it('is ok with both escapes set, and says so twice', () => {
    const r = preflight({ task: null, noTask: true, reviewers: [], noReviewer: true });
    expect(r).toEqual({ ok: true, notes: ['No task (n/a)', 'No reviewer (n/a)'] });
  });

  it('reports both blocks together', () => {
    const r = preflight({ task: null, noTask: false, reviewers: [], noReviewer: false });
    expect(r).toEqual({ ok: false, notes: ['Link a task (policy)', 'Add a reviewer (policy)'] });
  });
});
