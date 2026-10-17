import { describe, it, expect } from 'vitest';
import { parseMapUrl, isMapShortLink } from '../composables/useLocationPicker';

describe('parseMapUrl', () => {
  it('reads where a Share short link leads: /place/LAT,LNG with the pin in data', () => {
    // What https://maps.app.goo.gl/n3xNtsmCEmir9bdR6 redirects to, trimmed.
    const r = parseMapUrl('https://www.google.com/maps/place/21.061128,105.706235/data=!4m6!3m5!1s0!7e2!8m2!3d21.0611277!4d105.7062354!18m1!1e1?entry=gps&g_st=ac');
    expect(r).toEqual({ lat: 21.0611277, lng: 105.7062354, provider: 'google' });
  });

  it('takes the pin over the view centre, and the place name as label', () => {
    const r = parseMapUrl('https://www.google.com/maps/place/Nh%C3%A0+h%C3%A1t+L%E1%BB%9Bn/@21.0240,105.8550,17z/data=!3m1!4b1!4m6!3m5!1s0x0:0x0!8m2!3d21.0242!4d105.8576');
    expect(r).toEqual({ lat: 21.0242, lng: 105.8576, label: 'Nhà hát Lớn', provider: 'google' });
  });

  it('still reads ?q=, ?query= and @', () => {
    expect(parseMapUrl('https://www.google.com/maps?q=21.03,105.85')).toMatchObject({ lat: 21.03, lng: 105.85 });
    expect(parseMapUrl('https://www.google.com/maps/search/?api=1&query=21.03,105.85')).toMatchObject({ lat: 21.03, lng: 105.85 });
    expect(parseMapUrl('https://www.google.com/maps/@21.03,105.85,15z')).toMatchObject({ lat: 21.03, lng: 105.85 });
    expect(parseMapUrl('https://maps.google.com/?ll=21.03,105.85')).toMatchObject({ lat: 21.03, lng: 105.85 });
  });

  it('reads OSM links', () => {
    expect(parseMapUrl('https://www.openstreetmap.org/?mlat=21.03&mlon=105.85')).toMatchObject({ lat: 21.03, lng: 105.85, provider: 'osm' });
  });

  it('cannot read a short link by itself', () => {
    expect(parseMapUrl('https://maps.app.goo.gl/n3xNtsmCEmir9bdR6?g_st=ac')).toBeNull();
  });
});

describe('isMapShortLink', () => {
  it('knows the Share links and nothing else', () => {
    expect(isMapShortLink('https://maps.app.goo.gl/n3xNtsmCEmir9bdR6?g_st=ac')).toBe(true);
    expect(isMapShortLink('  https://goo.gl/maps/abc ')).toBe(true);
    expect(isMapShortLink('https://goo.gl/other')).toBe(false);
    expect(isMapShortLink('https://www.google.com/maps?q=1,2')).toBe(false);
    expect(isMapShortLink('Hanoi Opera House')).toBe(false);
  });
});
