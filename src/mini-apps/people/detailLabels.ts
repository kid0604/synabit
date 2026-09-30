/**
 * What a contact detail's label says on screen, as opposed to what is stored.
 *
 * The preset labels — "Email", "Phone", "How We Met" … — are saved in English
 * and other code finds details by that text (`includes('how we met')`, the
 * type inferred from "email"). Translating what is stored would break both,
 * and every contact already written. So the stored word stays the key, and
 * only its display is translated. Anything a person typed themselves, and
 * brand names, are shown as they are.
 */
const KEYS: Record<string, string> = {
  email: 'people.detail_email',
  phone: 'people.detail_phone',
  location: 'people.detail_location',
  'how we met': 'people.how_we_met',
  website: 'people.website',
  company: 'people.company',
};

export const detailLabel = (label: string, t: (key: string) => string): string => {
  const key = KEYS[label.trim().toLowerCase()];
  return key ? t(key) : label;
};
