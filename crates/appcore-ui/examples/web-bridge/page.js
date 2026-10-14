import { connectUiWeb, defineUiQuery, defineUiCommand } from './client/index.js';
const status = document.querySelector('#status');
const client = connectUiWeb();
const inspect = defineUiQuery({ capability: 'state.inspect', encode: () => null,
  decode: value => { if (!value || !Number.isSafeInteger(value.effects)) throw new Error('invalid'); return value.effects; } });
const change = defineUiCommand({ capability: 'state.change', encode: () => ({}),
  decode: value => { if (!value || value.accepted !== true) throw new Error('invalid'); return true; } });
const raw = window.webkit.messageHandlers.appcoreUiV1;
const boundary = {version:1,surface_id:window.appcoreUiV1.surface_id,request_id:'boundary',timeout_ms:1000,
  action:{kind:'query',query:{query_name:'state.inspect',query_id:'boundary',payload:null}}};
let rejected=0;
for (const [frame,code] of [[{...boundary,principal:'forged'},'invalid_frame'],[{...boundary,surface_id:'other'},'forbidden']]) {
  try { await raw.postMessage(JSON.stringify(frame)); } catch(error) { if(error.message===code) rejected++; }
}
document.querySelector('#boundary').textContent = `Pedidos indevidos rejeitados: ${rejected}/2`;
const subscription = await client.subscribe("state.inspect", "state.changed");
let notifications = 0;
const key = Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, "0")).join("");
async function refresh() {
  try { status.textContent = `Runtime ligado · efeitos: ${await client.query(inspect, null)}`; }
  catch (error) { status.textContent = `Falha controlada: ${error.code ?? 'backend_failure'}`; }
}
document.querySelector('#query').addEventListener('click', refresh);
document.querySelector('#command').addEventListener('click', async () => {
  try {
    await client.command(change, {}, { idempotencyKey: key });
    await refresh();
    const page = await subscription.poll(); notifications += page.events.length;
    document.querySelector('#events').textContent = `Notificações: ${notifications}`;
  }
  catch (error) { status.textContent = `Falha controlada: ${error.code ?? 'backend_failure'}`; }
});
void refresh();
