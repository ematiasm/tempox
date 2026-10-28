import { chromium } from '@playwright/test';
const b = await chromium.launch();
const p = await b.newPage();
await p.goto('http://localhost:5173/accounts');
await p.waitForSelector('li.row');
const rows = await p.evaluate(() => [...document.querySelectorAll('li.row')].map(r => ({
  cls: r.className,
  name: r.querySelector('.name')?.textContent?.trim(),
  figure: r.querySelector('.figure')?.textContent?.trim(),
  figures: [...r.querySelectorAll('.figure')].map(f => f.textContent.trim()),
})));
console.log(JSON.stringify(rows, null, 2));
const api = await (await fetch('http://localhost:3001/api/accounts')).json();
console.log('API:', api.map(a => `${a.name}=${a.balance}${a.is_default?' (default)':''}`).join(', '));
await b.close();
