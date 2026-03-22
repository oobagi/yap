import { mount } from 'svelte';
import History from './lib/history/History.svelte';

const app = mount(History, {
  target: document.getElementById('history')!,
});

export default app;
