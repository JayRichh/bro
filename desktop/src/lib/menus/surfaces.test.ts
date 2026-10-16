import { describe, expect, it } from 'vitest';
import { joinPath } from './surfaces';

describe('joinPath', () => {
  it('joins a Windows root and a relative path with backslashes', () => {
    expect(joinPath('C:\\code\\app', 'src/a.ts')).toBe('C:\\code\\app\\src\\a.ts');
    expect(joinPath('C:\\code\\app', 'src/lib/a.ts')).toBe('C:\\code\\app\\src\\lib\\a.ts');
  });

  it('joins a POSIX root and a relative path with slashes', () => {
    expect(joinPath('/home/me/app', 'src/a.ts')).toBe('/home/me/app/src/a.ts');
  });

  it('keeps forward slashes for a Windows root written with them', () => {
    expect(joinPath('C:/code/app', 'src/a.ts')).toBe('C:/code/app/src/a.ts');
  });

  it('does not double a trailing separator on the root', () => {
    expect(joinPath('C:\\code\\app\\', 'src/a.ts')).toBe('C:\\code\\app\\src\\a.ts');
    expect(joinPath('/home/me/app/', 'src/a.ts')).toBe('/home/me/app/src/a.ts');
    expect(joinPath('C:/code/app/', 'src/a.ts')).toBe('C:/code/app/src/a.ts');
  });

  it('takes the root separators for a relative path that uses the other kind', () => {
    expect(joinPath('/home/me/app', 'src\\a.ts')).toBe('/home/me/app/src/a.ts');
    expect(joinPath('C:\\code\\app', 'src\\a.ts')).toBe('C:\\code\\app\\src\\a.ts');
  });

  it('puts a file at the root of the repo directly under it', () => {
    expect(joinPath('/home/me/app', 'README.md')).toBe('/home/me/app/README.md');
    expect(joinPath('C:\\code\\app', 'README.md')).toBe('C:\\code\\app\\README.md');
  });
});
