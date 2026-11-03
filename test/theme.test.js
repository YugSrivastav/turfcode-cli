import test from 'node:test';
import assert from 'node:assert/strict';
import { PALETTE, TAGS, BLESSED_STYLES, CHALK } from '../src/theme.js';

test('Theme PALETTE defines required Electric Lime tokens', () => {
  assert.equal(PALETTE.bg, '#081008', 'Background must be obsidian-emerald (#081008)');
  assert.equal(PALETTE.electricLime, '#39FF14', 'Electric lime must be #39FF14');
  assert.equal(PALETTE.mutedBorder, '#1b3b1b', 'Muted border must be #1b3b1b');
  assert.equal(PALETTE.activeBorder, '#39FF14', 'Active border must be #39FF14');
});

test('Theme TAGS formats tags with TrueColor tags', () => {
  const limeText = TAGS.lime('test');
  assert.match(limeText, /#39ff14-fg/, 'TAGS.lime should apply #39ff14-fg');

  const badgeActive = TAGS.badge('ACTIVE', 'active');
  assert.match(badgeActive, /#39ff14-fg/, 'Active badge should use electric lime');

  const badgeLocked = TAGS.badge('LOCKED', 'locked');
  assert.match(badgeLocked, /🔒/, 'Locked badge should include padlock icon');
  assert.match(badgeLocked, /#ef4444-fg/, 'Locked badge should use red');

  const badgeProgress = TAGS.badge('IN PROGRESS', 'in_progress');
  assert.match(badgeProgress, /#eab308-fg/, 'Progress badge should use amber/yellow');
});

test('BLESSED_STYLES defines correct widget styles', () => {
  assert.equal(BLESSED_STYLES.sidebar.width, '30%', 'Sidebar default width must be 30%');
  assert.equal(BLESSED_STYLES.terminalPane.width, '70%', 'Terminal pane default width must be 70%');
  assert.equal(BLESSED_STYLES.screen.style.bg, '#081008');
  assert.ok(CHALK.lime, 'Chalk lime helper must exist');
});
