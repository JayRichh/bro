import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { toast } from './lib/store.svelte';

// A release build has no devtools: say what broke in the window itself.
window.addEventListener('error', (e) => toast(`Error: ${e.message}`));
window.addEventListener('unhandledrejection', (e) => toast(`Error: ${String(e.reason)}`));

mount(App, { target: document.getElementById('app')! });
