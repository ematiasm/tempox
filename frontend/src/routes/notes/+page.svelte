<script lang="ts">
  import { onMount } from 'svelte';
  import { createNote, deleteNote, listNotes, type Note } from '#lib/api.js';
  import { lang } from '#lib/i18n/index.svelte.js';

  let notes = $state<Note[]>([]);
  let text = $state('');
  let error = $state<string | null>(null);

  async function refresh() {
    try {
      notes = await listNotes();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function add() {
    const t = text.trim();
    if (!t) return;
    try {
      await createNote(t);
      text = '';
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function remove(id: string) {
    try {
      await deleteNote(id);
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  onMount(refresh);
</script>

<h1 class="text-2xl font-bold mb-4">{lang.t.notes_title}</h1>

<div class="flex gap-2 mb-6">
  <input
    class="bg-slate-900 border border-slate-700 rounded px-3 py-2 flex-1"
    placeholder={lang.t.notes_placeholder}
    bind:value={text}
    onkeydown={(e) => e.key === 'Enter' && add()}
  />
  <button class="bg-emerald-600 hover:bg-emerald-500 rounded px-4 py-2 font-semibold" onclick={add}>
    {lang.t.notes_add}
  </button>
</div>

{#if error}
  <p class="text-red-400 mb-4">{error}</p>
{/if}

{#if notes.length === 0}
  <p class="text-slate-500 italic">{lang.t.notes_empty}</p>
{:else}
  <ul class="space-y-2">
    {#each notes as n (n.id)}
      <li class="flex items-center justify-between border border-slate-800 rounded px-3 py-2">
        <span>{n.text}</span>
        <button class="text-red-400 hover:text-red-300 text-sm" onclick={() => remove(n.id)}>
          ✕
        </button>
      </li>
    {/each}
  </ul>
{/if}
