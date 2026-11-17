# Recommendations

> Part of the [MFE Player](../README.md) manual.

MFE's radio is a local, single-user version of the multi-stage stack Spotify
describes in its engineering papers. The stages are the same; the data is what
one machine can honestly know.

This page explains what runs, what it learns from, how to see it, and where it
stops being Spotify.

---

## Why a stack instead of one algorithm

One score cannot answer both *"what is similar to this song?"* and *"what do I
want at 8am on a Sunday?"*. Spotify's system is layered because each layer
solves a different problem:

| Layer | Spotify (public research) | MFE, locally |
|---|---|---|
| **Implicit feedback** | streams, saves, playlist adds, completion, early skips | plays, completions, likes, repeats, skips, quick skips, dislikes |
| **Latent model** | Logistic Matrix Factorization (Johnson, Spotify 2014); ANN retrieval (Annoy, later Voyager/HNSW) | online logistic model over tag/artist/context features; exact cosine search (the catalogue is already in memory) |
| **Content — text** | NLP over web pages, lyrics and cultural chatter | metadata genres, plugin tags, title/album tokens (edit stopwords stripped) |
| **Content — audio** | Echo Nest features, CNNs on spectrograms | tag energy heuristic: tempo/energy pacing & DJ flow continuity |
| **Relational** | playlist/user/song graph embeddings (node2vec) | co-listening graph + directed Markov transition model $P(B \mid A)$ |
| **Context** | CoSeRNN session embeddings: day, hour, device | time-of-day bucket + weekday/weekend, fused with the session profile |
| **Ranking** | BaRT contextual bandits, epsilon-greedy / Thompson / UCB, RL rankers | Beta-Bernoulli arms per genre with Thompson sampling + epsilon-greedy exploration, skip-streak recovery mode, and weighted blend |
| **Ordering** | generate several shuffle sequences, score freshness and artist spacing (20–30% of playlist length) | three sampled sequences scored for freshness, artist spacing and genre runs; anchor-first batch rhythm |
| **Quality** | 2026 slop/spam exclusion, AI-persona badges | title/artist screen for slowed/reverb/sped/nightcore/8D/AI-cover/loop/chopped re-uploads |

The important design choice is that the layers stay separate. A skipped genre
moves a session veto and a bandit arm; a finished track moves the logistic
model, the context bucket and the co-listening graph. Nothing is one opaque
score.

---

## The stages, in the order they run

### 1. Implicit feedback

Every track start opens an observation. When it ends, MFE converts the outcome
into a continuous reward:

| Outcome | Reward |
|---|---|
| like / repeat / natural end | 1.0 |
| ended after 50–90% | 0.6, scaled by how much was heard |
| skipped after ≥75% | 0.6 |
| soft skip (40–75%) | 0.25 |
| skip (under 40%, or before 40s) | 0.05 |
| quick skip (<15s) | 0.0 |
| dislike | 0.0 + a 30-day block |
| playback error | no signal |

Rewards are continuous on purpose: Spotify's bandit papers describe rewards
derived from streaming time, not just clicks. A track that plays to 92% is a
stronger signal than one that ends at 51%.

### 2. Retrieval

Candidates come from the metadata plugins, as before: genre searches built
from the profile (`"phonk mix"`, top artists), then the current artist/title.
Autoplay still refuses live streams, news/talk/podcast titles and anything
outside 15 seconds–20 minutes, and it never autoplays live radio.

### 3. Content score

`tags_for()` builds a sparse weighted vector per track: genres (1.2), plugin
tags (0.9), title tokens (0.3), album tokens (0.1), source and decade. Cosine
similarity against the long-term profile is the content term.

This is where MFE is *not* Spotify: there is no audio CNN and no web NLP. The
tags come from metadata, not from listening to the waveform or reading the
internet.

### 4. Behavioural score & Markov transitions

Every consecutive pair of plays adds an undirected co-listening edge, track→track
and artist→artist, as well as a directed Markov transition edge $P(B \mid A)$.
Finishing track $B$ after $A$ strengthens the transition probability. Recommendations
score higher when the candidate naturally follows the preceding track or artist in
the listener's history.

This is the local substitute for collaborative filtering and sequence prediction.
On a shared service the graph comes from millions of users; here it comes from
this machine's own verified listening paths.

### 5. Energy pacing & DJ flow

Transitions between tracks evaluate relative acoustic energy estimated from genre and
acoustic metadata tags (`track_energy`). Smooth transitions ($\Delta \le 0.35$)
receive a positive pacing bonus ($+0.06$). Jarring leaps ($\Delta > 0.70$) receive a
penalty (up to $-0.15$), preventing sudden harsh disruptions (such as jumping
from ambient directly into thrash metal or hardstyle).

### 6. Logistic model

`Signals.weights` is a first-order logistic model trained online with SGD:

```
p(reward) = sigmoid(bias + Σ w_tag·tag + Σ w_context,tag·tag + w_artist)
```

That is the same objective shape as Logistic MF (a logistic link over latent
features) collapsed to one user and hashed metadata features — the honest
single-user version. Confidence comes from the reward, so repeated
completions move a weight faster than one play.

### 7. Context

At the start of playback MFE reads the local hour and weekday, then buckets it
the way CoSeRNN does: early morning, morning, afternoon, evening, late evening,
night. Affinities are learned per bucket, so a genre that works at 9am is not
assumed to work at 1am.

Unix gets the real local hour; Windows falls back to UTC unless
`MFE_CONTEXT_HOUR` is set. The context is a soft re-weight, never a hard
filter.

### 8. Bandit rerank & Skip-streak recovery mode

Each genre is a Beta-Bernoulli arm. A reward increments `alpha`, a non-reward
increments `beta`. Ranking samples a normal approximation of the Beta
posterior (Thompson-style), so an arm with two trials is allowed to win once
in a while and an arm with forty trials has to earn it. `autoplay_explore`
adds the epsilon-greedy share: a small chance to favour an under-explored arm,
which is the mechanism BaRT uses to keep buying information instead of always
exploiting.

**Skip-Streak Recovery Mode:** If the listener skips 2 or more tracks consecutively
(`consecutive_skips >= 2`), the algorithm detects listener fatigue. Exploration
is immediately dampened to 0.0, and a **+0.28 anchor bonus** is awarded to
high-affinity core tracks (while penalizing unfamiliar tracks) to quickly stabilize
the session before resuming discovery.

### 9. Blend, anchor rhythm, and MMR diversity

The candidate score is calculated as:

```
0.22 content + 0.15 logistic + 0.13 behaviour + 0.12 context
+ 0.09 artist affinity + 0.08 transition + 0.06 novelty + 0.05 bandit + 0.10 freshness
+ pacing + anchor_bonus
- 1.2 × session veto
```

The batch is planned with an **anchor-vs-discovery rhythm**: slot 0 (and slot 3 for
batches of 5+) guarantees familiar anchor tracks so discovery probes do not
monopolize sequential listening. The rest is picked with max-marginal-relevance (MMR)
diversity: a track loses value if it is too similar to something already chosen,
an artist is penalized if repeated closely, and session vetoes (-2.0 after repeated skips)
remain decisive.

### 10. Scored shuffle

`shuffle` is not `rand()`. Enabling shuffle (or pressing `s`) rescores the
upcoming queue:

- **freshness** — recently heard tracks are penalised hardest at the front;
- **artist spacing** — gaps below ~20% of the queue are penalised, gaps around
  25% get a small bonus, and spacing is intentionally not perfectly even;
- **genre runs** — consecutive tracks with the same primary tag lose points.

Three random sequences are generated and the best-scoring one wins. During
playback, `next` samples four remaining tracks and keeps the one that fits the
last few plays, so the queue order stays stable for `J`/`K` editing.

### 11. Quality screen & Edit token exclusion

`taste_filter = true` drops titles that are re-upload slop before they can be
ranked: slowed/reverb variations (`slowed & reverb`, `slowed - reverb`, `slowed down`),
sped-up, nightcore, daycore, 8D, bass-boosted, chopped & screwed, 1/10-hour loops,
AI covers and synthesized uploads.

Furthermore, tokenization strips edit buzzwords (`slowed`, `reverb`, `sped`,
`nightcore`, `daycore`, `boosted`, etc.) so secondary re-upload tags never pollute
the listener's taste profile. The original tracks stay searchable and playable by
hand; slop variations simply do not get recommended.

---

## Seeing and steering it

```bash
mfe taste status      # session confidence, top tags/artists, last pick
mfe taste explain     # every component of the last pick, including bandit
mfe taste profile     # the whole model: arms, context bucket, graph size
mfe taste contexts    # what each time-of-day bucket has learned
mfe taste steer more synthwave   # nudge the profile now
mfe taste steer less phonk
mfe taste filter on|off
mfe taste reset session|probes|contexts|all
```

`mfe taste profile` is the closest local equivalent of Spotify's 2026 Taste
Profile: it shows the inferred genres, artists, contexts and arm posteriors
instead of hiding them behind a playlist.

---

## Privacy and storage

Everything lives in `~/.config/mfe-player/taste.json`:

- long-term and session affinities,
- contextual affinities per bucket,
- the co-listening graph,
- bandit arm posteriors,
- logistic weights.

No account, no network calls, no cross-device profile. `listenbrainz_token`
remains the only optional outbound scrobble and is documented separately.

## Cold start

With no history, the profile is empty and autoplay falls back to the
`similar` radio path (artist/title searches). After roughly eight plays, the
confidence gate opens and probes begin. A reset clears the model but remembers
that it was reset, so history is not silently re-imported.

## Where this stops

- **No other users.** Logistic MF and node2vec at Spotify learn from millions
  of users and playlists; the local graph is one listener.
- **No audio model.** No spectrogram CNN, no Echo Nest features.
- **No web NLP.** Tags are metadata, not blog sentiment.
- **No server-side A/B testing.** The bandit is real, but it optimises this
  device's listening, not a population.
- **"ANN" is exact search.** For a terminal-sized result set, scanning vectors
  is faster and always correct.
