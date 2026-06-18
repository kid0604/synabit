import { ref, computed } from 'vue';
import type { Editor } from '@tiptap/vue-3';

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

  /** Parse Google Maps / OSM URL → {lat, lng, provider} */
  const parseMapUrl = (url: string): { lat: number; lng: number; label?: string; provider: 'osm' | 'google' } | null => {
    try {
      // Google Maps: various formats
      // https://www.google.com/maps?q=LAT,LNG
      // https://www.google.com/maps/@LAT,LNG,ZOOMz
      // https://maps.google.com/?ll=LAT,LNG
      // https://goo.gl/maps/... (short link, won't parse)
      const u = new URL(url);
      if (u.hostname.includes('google.com') || u.hostname.includes('maps.google')) {
        const q = u.searchParams.get('q');
        if (q) {
          const parts = q.split(',').map(s => parseFloat(s.trim()));
          if (parts.length >= 2 && !isNaN(parts[0]) && !isNaN(parts[1])) {
            return { lat: parts[0], lng: parts[1], provider: 'google' };
          }
        }
        const atMatch = u.pathname.match(/@(-?\d+\.\d+),(-?\d+\.\d+)/);
        if (atMatch) {
          return { lat: parseFloat(atMatch[1]), lng: parseFloat(atMatch[2]), provider: 'google' };
        }
        const placeMatch = u.pathname.match(/place\/([^/]+)\//);
        if (placeMatch && atMatch) {
          return { lat: parseFloat(atMatch[1]), lng: parseFloat(atMatch[2]), label: decodeURIComponent(placeMatch[1].replace(/\+/g, ' ')), provider: 'google' };
        }
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
      locationModal.value.error = 'Could not search. Check your internet connection.';
      locationModal.value.suggestions = [];
    } finally {
      locationModal.value.searching = false;
    }
  };

  const onLocationInput = (val: string) => {
    locationModal.value.input = val;
    locationModal.value.error = '';
    locationModal.value.suggestions = [];

    // Try URL first
    const urlResult = parseMapUrl(val);
    if (urlResult) {
      locationModal.value.lat = urlResult.lat;
      locationModal.value.lng = urlResult.lng;
      locationModal.value.provider = urlResult.provider;
      if (urlResult.label) locationModal.value.label = urlResult.label;
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
    if (geocodeTimer) clearTimeout(geocodeTimer);
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
    let label = r.label || 'Directions';
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
