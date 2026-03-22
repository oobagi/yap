import { mount } from 'svelte';
import Settings from './lib/settings/Settings.svelte';

const app = mount(Settings, {
  target: document.getElementById('settings')!,
});

export default app;
