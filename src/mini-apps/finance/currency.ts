import { ref } from 'vue';

export const currentCurrency = ref('USD');

export const formatCurrency = (val: number) => {
    let locale = 'en-US';
    if (currentCurrency.value === 'VND') locale = 'vi-VN';
    else if (currentCurrency.value === 'EUR') locale = 'de-DE';
    else if (currentCurrency.value === 'JPY') locale = 'ja-JP';
    else if (currentCurrency.value === 'GBP') locale = 'en-GB';

    return new Intl.NumberFormat(locale, { style: 'currency', currency: currentCurrency.value }).format(val);
};

const CACHE_KEY = 'synabit_exchange_rates';

export const fetchExchangeRate = async (fromCurrency: string, toCurrency: string): Promise<number | null> => {
    const from = fromCurrency.toLowerCase();
    const to = toCurrency.toLowerCase();
    if (from === to) return 1;
    
    let cachedRates: any = {};
    try {
        const cacheStr = localStorage.getItem(CACHE_KEY);
        if (cacheStr) cachedRates = JSON.parse(cacheStr);
    } catch(e) {}

    try {
        const response = await fetch(`https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@latest/v1/currencies/${from}.json`);
        if (!response.ok) throw new Error('Failed to fetch exchange rate');
        
        const data = await response.json();
        
        // Update cache
        cachedRates[from] = data[from];
        localStorage.setItem(CACHE_KEY, JSON.stringify(cachedRates));
        
        if (data && data[from] && data[from][to]) {
            return data[from][to];
        }
        return null;
    } catch (e) {
        console.error('Error fetching exchange rate, attempting to use cache:', e);
        // Fallback to cache if network fails
        if (cachedRates && cachedRates[from] && cachedRates[from][to]) {
            return cachedRates[from][to];
        }
        return null;
    }
};
