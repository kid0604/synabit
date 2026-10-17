import { ref, computed } from 'vue';
import type { Editor } from '@tiptap/vue-3';
import { invoke } from '@tauri-apps/api/core';
import { i18n } from '../../../../i18n';

export interface LocationModalState {
  show: boolean;
  input: string;
  lat: number | null;
  lng: number | null;
  label: string;
  provider: 'osm' | 'google';
  searching: boolean;
  suggestions: { display: string; lat: number; lng: number }[];
  error: string;
}

export interface RouteModalState {
  show: boolean;
  urlInput: string;
  error: string;
  label: string;
}

export interface ParsedMapUrl {
  lat: number;
  lng: number;
  label?: string;
  provider: 'osm' | 'google';
}

const coordPair = (text: string | null | undefined): { lat: number; lng: number } | null => {
  const m = text?.trim().match(/^(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)$/);
  if (!m) return null;
  const lat = parseFloat(m[1]);
  const lng = parseFloat(m[2]);
  return lat >= -90 && lat <= 90 && lng >= -180 && lng <= 180 ? { lat, lng } : null;
};

/** A Share link from Google Maps: no coordinates until its redirect is read. */
export const isMapShortLink = (url: string): boolean => {
  try {
    const u = new URL(url.trim());
    return u.hostname === 'maps.app.goo.gl' || (u.hostname === 'goo.gl' && u.pathname.startsWith('/maps'));
  } catch { return false; }
};

/** Parse Google Maps / OSM URL → {lat, lng, provider} */
export const parseMapUrl = (url: string): ParsedMapUrl | null => {
  try {
    const u = new URL(url.trim());
    if (u.hostname.includes('google.') && (u.hostname.startsWith('maps.') || u.pathname.startsWith('/maps'))) {
      const path = decodeURIComponent(u.pathname);
      // /maps/place/NAME/… — a name, unless the "name" is the coordinates
      const placeName = path.match(/\/place\/([^/]+)/)?.[1].replace(/\+/g, ' ');
      const label = placeName && !coordPair(placeName) ? placeName : undefined;
      const found = (c: { lat: number; lng: number } | null) => c && { ...c, provider: 'google' as const, ...(label ? { label } : {}) };

      // …/data=…!3dLAT!4dLNG — the pin itself, where @ is only the view's centre
      const pin = path.match(/!3d(-?\d+(?:\.\d+)?)!4d(-?\d+(?:\.\d+)?)/);
      if (pin) return found(coordPair(`${pin[1]},${pin[2]}`));
      // /maps/place/LAT,LNG or /maps/search/LAT,LNG
      const inPath = path.match(/\/(?:place|search)\/([^/]+)/)?.[1];
      if (coordPair(inPath)) return found(coordPair(inPath));
      // ?q=LAT,LNG, ?ll=LAT,LNG, ?query=LAT,LNG (api=1)
      for (const key of ['q', 'query', 'll']) {
        const c = coordPair(u.searchParams.get(key));
        if (c) return found(c);
      }
      // /maps/@LAT,LNG,ZOOMz
      const at = path.match(/@(-?\d+\.\d+),(-?\d+\.\d+)/);
      if (at) return found(coordPair(`${at[1]},${at[2]}`));
    }
    // OpenStreetMap: https://www.openstreetmap.org/?mlat=LAT&mlon=LNG
    if (u.hostname.includes('openstreetmap.org')) {
      const mlat = u.searchParams.get('mlat');
      const mlon = u.searchParams.get('mlon');
      if (mlat && mlon) {
        return { lat: parseFloat(mlat), lng: parseFloat(mlon), provider: 'osm' };
      }
      const hash = u.hash; // #map=ZOOM/LAT/LNG
      const hashMatch = hash.match(/#map=\d+\/(-?\d+\.\d+)\/(-?\d+\.\d+)/);
      if (hashMatch) {
        return { lat: parseFloat(hashMatch[1]), lng: parseFloat(hashMatch[2]), provider: 'osm' };
      }
    }
  } catch { /* not a URL */ }
  return null;
};

export function useLocationPicker() {
  const editorRef = ref<Editor | null>(null);

  const setEditor = (editor: Editor | null | undefined) => {
    editorRef.value = editor ?? null;
  };

  // --- Location prompt ---
  const locationModal = ref<LocationModalState>({
    show: false, input: '', lat: null, lng: null, label: '',
    provider: 'osm', searching: false, suggestions: [], error: ''
  });

  let geocodeTimer: ReturnType<typeof setTimeout> | null = null;

  /** Parse raw lat,lng string */
  const parseLatLng = (input: string): { lat: number; lng: number } | null => {
    const match = input.trim().match(/^(-?\d+\.?\d*)\s*[,\s]\s*(-?\d+\.?\d*)$/);
    if (match) {
      const lat = parseFloat(match[1]);
      const lng = parseFloat(match[2]);
      if (lat >= -90 && lat <= 90 && lng >= -180 && lng <= 180) {
        return { lat, lng };
      }
    }
    return null;
  };

  /** Nominatim geocoding (free, privacy-first, 1 req/s) */
  const geocodeAddress = async (query: string) => {
    if (query.length < 3) {
      locationModal.value.suggestions = [];
      return;
    }
    locationModal.value.searching = true;
    locationModal.value.error = '';
    try {
      const res = await fetch(
        `https://nominatim.openstreetmap.org/search?format=json&q=${encodeURIComponent(query)}&limit=5&addressdetails=1`,
        { headers: { 'User-Agent': 'Synabit/0.4.1 (https://synabit.app)' } }
      );
      if (!res.ok) throw new Error('Geocoding failed');
      const data = await res.json();
      locationModal.value.suggestions = data.map((item: any) => ({
        display: item.display_name,
        lat: parseFloat(item.lat),
        lng: parseFloat(item.lon),
      }));
    } catch (e: any) {
      locationModal.value.error = i18n.global.t('note.editor.location.search_failed');
      locationModal.value.suggestions = [];
    } finally {
      locationModal.value.searching = false;
    }
  };

  const applyUrlResult = (r: ParsedMapUrl) => {
    locationModal.value.lat = r.lat;
    locationModal.value.lng = r.lng;
    locationModal.value.provider = r.provider;
    if (r.label && !locationModal.value.label) locationModal.value.label = r.label;
  };

  /** Short links are resolved natively; an answer for an input since replaced is dropped. */
  const resolveShortLink = async (val: string) => {
    locationModal.value.lat = null;
    locationModal.value.lng = null;
    locationModal.value.searching = true;
    try {
      const resolved = parseMapUrl(await invoke<string>('resolve_map_link', { url: val.trim() }));
      if (locationModal.value.input !== val) return;
      if (resolved) applyUrlResult(resolved);
      else locationModal.value.error = i18n.global.t('note.editor.location.link_unreadable');
    } catch {
      if (locationModal.value.input === val) {
        locationModal.value.error = i18n.global.t('note.editor.location.link_failed');
      }
    } finally {
      if (locationModal.value.input === val) locationModal.value.searching = false;
    }
  };

  const onLocationInput = (val: string) => {
    locationModal.value.input = val;
    locationModal.value.error = '';
    locationModal.value.suggestions = [];
    locationModal.value.searching = false;
    if (geocodeTimer) clearTimeout(geocodeTimer);

    // Try URL first
    const urlResult = parseMapUrl(val);
    if (urlResult) {
      applyUrlResult(urlResult);
      return;
    }
    if (isMapShortLink(val)) {
      void resolveShortLink(val);
      return;
    }

    // Try lat,lng
    const coordResult = parseLatLng(val);
    if (coordResult) {
      locationModal.value.lat = coordResult.lat;
      locationModal.value.lng = coordResult.lng;
      return;
    }

    // Otherwise treat as address search (debounced)
    locationModal.value.lat = null;
    locationModal.value.lng = null;
    geocodeTimer = setTimeout(() => geocodeAddress(val), 500);
  };

  const selectSuggestion = (s: { display: string; lat: number; lng: number }) => {
    locationModal.value.lat = s.lat;
    locationModal.value.lng = s.lng;
    locationModal.value.label = s.display.split(',').slice(0, 2).join(',').trim();
    locationModal.value.suggestions = [];
    locationModal.value.input = s.display;
  };

  const confirmLocation = () => {
    if (!editorRef.value || locationModal.value.lat === null || locationModal.value.lng === null) return;
    editorRef.value.commands.setLocation({
      lat: locationModal.value.lat,
      lng: locationModal.value.lng,
      label: locationModal.value.label || '',
      zoom: 15,
      provider: locationModal.value.provider,
    });
    locationModal.value = {
      show: false, input: '', lat: null, lng: null, label: '',
      provider: 'osm', searching: false, suggestions: [], error: ''
    };
  };

  // --- Route Modal ---
  const routeModal = ref<RouteModalState>({
    show: false, urlInput: '', error: '', label: ''
  });

  const isValidRouteUrl = computed(() => {
    try {
      const u = new URL(routeModal.value.urlInput.trim());
      const isGoogle = (u.hostname.includes('google.com') || u.hostname.includes('maps.google') || u.hostname.includes('goo.gl'))
        && (u.pathname.includes('/dir') || u.pathname.includes('/maps'));
      const isOSM = u.hostname.includes('openstreetmap.org') && (u.pathname.includes('/directions') || u.searchParams.has('route'));
      return isGoogle || isOSM;
    } catch { return false; }
  });

  /** Detect provider from URL */
  const detectRouteProvider = (url: string): 'google' | 'osm' => {
    try {
      const u = new URL(url);
      if (u.hostname.includes('openstreetmap.org')) return 'osm';
    } catch { /* default */ }
    return 'google';
  };

  const confirmRoute = () => {
    const r = routeModal.value;
    if (!editorRef.value || !isValidRouteUrl.value) return;
    const url = r.urlInput.trim();
    const provider = detectRouteProvider(url);

    // Extract label from URL
    let label = r.label || i18n.global.t('note.editor.location.directions');
    if (!r.label) {
      try {
        const u = new URL(url);
        if (provider === 'google') {
          const parts = u.pathname.replace(/^\/maps\/dir\/?/, '').split('/').filter(p => p && !p.startsWith('@') && !p.startsWith('data'));
          if (parts.length >= 2) {
            const o = decodeURIComponent(parts[0]).replace(/\+/g, ' ');
            const d = decodeURIComponent(parts[1]).replace(/\+/g, ' ');
            label = `${o.split(',')[0].trim()} → ${d.split(',')[0].trim()}`;
          }
        } else {
          // OSM: route=lat1,lng1;lat2,lng2
          const route = u.searchParams.get('route');
          if (route) {
            const pts = route.split(';');
            if (pts.length >= 2) label = `${pts[0]} → ${pts[pts.length - 1]}`;
          }
        }
      } catch { /* use default */ }
    }
    editorRef.value.commands.setRoute({ routeUrl: url, label, provider });
    routeModal.value = { show: false, urlInput: '', error: '', label: '' };
  };

  return {
    locationModal,
    routeModal,
    isValidRouteUrl,
    onLocationInput,
    selectSuggestion,
    confirmLocation,
    confirmRoute,
    setEditor,
  };
}
