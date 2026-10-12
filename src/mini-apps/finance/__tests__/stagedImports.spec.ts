import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { i18n } from '../../../i18n';
import StagedImports from '../components/StagedImports.vue';
import { importRows, matchColumns, parseCsv } from '../csv';

/** As `syn::staging` writes it: Finance's own header, the bank's own cells. */
const STAGED = 'date,type,amount,category,account,note\r\n01/09/2026,,-55.000,Ăn uống,VCB,"HIGHLANDS COFFEE Q1, ""chi nhánh"""\r\n02/09/2026,,25.000.000,,VCB,Luong thang 8\r\n';

describe('What Syn staged', () => {
  it('reads through the same import as a file picked by hand', () => {
    const [header, ...body] = parseCsv(STAGED);
    const result = importRows(body, matchColumns(header), {
      accounts: [{ id: 'acc-vcb', name: 'VCB' } as never],
      categories: [{ id: 'food', name: 'Ăn uống' } as never],
      currency: 'VND',
      existing: [],
    });
    expect(result.problems).toEqual([]);
    expect(result.ready.map((t) => [t.type, t.amount, t.category, t.accountId])).toEqual([
      ['expense', 55000, 'food', 'acc-vcb'],
      ['income', 25000000, '', 'acc-vcb'],
    ]);
    expect(result.ready[0].note).toBe('HIGHLANDS COFFEE Q1, "chi nhánh"');
  });

  it('is offered above the ledger, its words quoted as Syn’s', async () => {
    const w = mount(StagedImports, {
      props: { staged: [{ id: 'finance-1', why: 'Sao kê tháng 9', source: 'r1 · sao-ke.csv', rows: 2, created_at: 'now' }] },
      global: { plugins: [i18n] },
    });
    expect(w.text()).toContain('“Sao kê tháng 9”');
    await w.find('[data-test="staged-preview"]').trigger('click');
    await w.find('[data-test="staged-discard"]').trigger('click');
    expect(w.emitted('preview')).toEqual([['finance-1']]);
    expect(w.emitted('discard')).toEqual([['finance-1']]);
  });
});
