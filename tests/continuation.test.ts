import { describe, expect, it } from 'vitest';
import { stripAutoContinuation } from '../src/utils/continuation';
describe('generated continuation metadata', () => {
 it('keeps edited names and original voice qualifiers without persisting automatic markers', () => {
  expect(stripAutoContinuation("新人物 (CONT'D)", true)).toBe('新人物');
  expect(stripAutoContinuation("人物（画外音, CONT'D）", true)).toBe('人物（画外音）');
 });
 it('preserves explicit user markers and dialogue', () => {
  expect(stripAutoContinuation("人物 (CONT'D)", false)).toBe("人物 (CONT'D)");
  expect(stripAutoContinuation('纸上写着 CONTD。', true)).toBe('纸上写着 CONTD。');
 });
});
