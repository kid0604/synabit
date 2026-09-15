import { describe, it, expect } from 'vitest';
import { lensFor, nodeVisible, linkVisible, isDeceased, type TimeFrame } from '../timeFrame';

const frame = (): TimeFrame => ({
  first_seen: { me: '2009-01-01', ha: '2013-10-19', quang: '2014-07-01', ba: '2009-01-01' },
  died_on: { ba: '2017-11-22' },
  links: [
    { source: 'me', target: 'quang', since: '2014-07-01', until: '2018-07-31' },
    { source: 'quang', target: 'me', since: '2021-01-01', until: null },
  ],
  density: [],
  earliest: '2009-01',
});

const links = [
  { source: 'me', target: 'ha' },
  { source: 'ha', target: 'tag-family' },
  { source: 'ba', target: 'tag-family' },
  { source: 'me', target: 'quang' },
];

describe('the time lens', () => {
  it('shows a node from the day it arrived', () => {
    const lens = lensFor(frame(), links);
    expect(nodeVisible(lens, 'ha', '2013-10-18')).toBe(false);
    expect(nodeVisible(lens, 'ha', '2013-10-19')).toBe(true);
  });

  it('dates a tag by the first thing that uses it', () => {
    const lens = lensFor(frame(), links);
    expect(nodeVisible(lens, 'tag-family', '2008-12-31')).toBe(false);
    expect(nodeVisible(lens, 'tag-family', '2009-01-01')).toBe(true);
  });

  it('shows a node it knows nothing about', () => {
    const lens = lensFor(frame(), links);
    expect(nodeVisible(lens, 'somewhere-else', '1990-01-01')).toBe(true);
  });

  it('holds a relationship only while one of its spans lasted', () => {
    const lens = lensFor(frame(), links);
    expect(linkVisible(lens, 'me', 'quang', '2016-01-01')).toBe(true);
    expect(linkVisible(lens, 'quang', 'me', '2019-06-01')).toBe(false);
    expect(linkVisible(lens, 'me', 'quang', '2022-01-01')).toBe(true);
    expect(linkVisible(lens, 'me', 'ha', '1990-01-01')).toBe(true);
  });

  it('knows who had died by a given day', () => {
    const lens = lensFor(frame(), links);
    expect(isDeceased(lens, 'ba', '2017-11-21')).toBe(false);
    expect(isDeceased(lens, 'ba', '2017-11-22')).toBe(true);
    expect(isDeceased(lens, 'me', '2026-01-01')).toBe(false);
  });
});
