# Tamlil QA · macOS arm64 · run 39944912024 · 737289c

Runner: arm64 Apple M1 (Virtual), 3 cores

- Wrote fbank of he_il/1620 (1606 frames) to `/Users/runner/work/Tamlil/Tamlil/qa-data/fbank-rust.json`

- Wrote 20 embeddings to `/Users/runner/work/Tamlil/Tamlil/qa-data/embeddings-rust.json`

- Loaded `ivrit-large-v3-turbo` in 9.3 s

## Language identification (first 8 s)

General model `tiny` vs the Hebrew fine-tune. 8/8 correct with the general model.

| clip | expected | tiny | time | ivrit-turbo | time |
|---|---|---|---|---|---|
| he_il 1620 | he | he (0.98) | 1.67 s | he (1.00) | 41.20 s |
| he_il 1617 | he | he (0.91) | 1.26 s | he (1.00) | 35.22 s |
| he_il 1592 | he | he (0.99) | 0.92 s | he (1.00) | 34.81 s |
| he_il 1651 | he | he (0.18) | 0.73 s | he (1.00) | 35.36 s |
| en_us 1548 | en | en (0.99) | 0.72 s | he (1.00) | 30.28 s |
| en_us 1620 | en | en (1.00) | 0.65 s | he (1.00) | 29.34 s |
| en_us 1510 | en | en (0.99) | 0.75 s | he (1.00) | 29.48 s |
| en_us 1578 | en | en (0.87) | 0.73 s | he (1.00) | 29.15 s |

- Loaded `ivrit-large-v3-turbo` in 10.0 s

## Simulated meeting through VAD + per-segment whisper (VAD variant default:0.50:300)

8 Hebrew clips, 69 s speech in 77 s of audio. VAD produced **10 segments** (mean 5.9 s, min 2.5 s, max 10.8 s) covering 59 s. Whisper compute 70 s → **RTF 1.00** vs speech. **WER 25.0%** (33 / 132).

| seg | start | s | text |
|---|---|---|---|
| 0 | 1.3 | 7.8 | כעת זמין באופן רחב בכל רחבי הארכי פלג, המטבח הג'ווני מאופיין במגוון מנות מטובלות בפשטות, והטעמים הבולטים |
| 1 | 9.2 | 2.5 | שהג'וונסנים מואביים הם בוטנים |
| 2 | 12.8 | 3.2 | פלפל צ'ילה, סוכר ומגוון טובלינים ארמוטי |
| 3 | 18.8 | 10.8 | נמצא בחלק הצפון-מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבאווי בצפון, וגבולו הדרומי הוא נהר קיאמבי, הפארק הלאומי קרוגר קרוקודייל. |
| 4 | 32.2 | 6.9 | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. |
| 5 | 40.9 | 5.0 | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. |
| 6 | 47.1 | 5.1 | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי |
| 7 | 53.7 | 7.5 | בבעלותה של יפן כמעט 7,000 אי אימה, שהגדול מביניהם הוא הונשו, מה שהופך את יפן לאי השבע בגודלו בעולם. |
| 8 | 62.6 | 5.0 | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפלפינים שולמו על ידי העם הפיליפיני עצמו. |
| 9 | 69.8 | 5.8 | FIC ווסט ניתן להמיר כסף בבנק היחידי בעיים שממוקם בסטנלי מול חנות. |

## Speaker identification (WeSpeaker CAM++)

- Embedding time per clip: 65 ms
- Same speaker (two halves of one clip): mean 0.74, min 0.55, max 0.89 (n=20)
- Different speakers (female vs male clips, any language): mean 0.08, min -0.15, max 0.31 (n=75)
- Conversation clustering (system channel, mic guess off): **6 distinct labels** for 2 speakers, same-clip consistency **90%**, 0 utterances too short to embed
- Same clip, two halves → same label: 9/10 clips
- Labels used by female voices: {"Speaker 1", "Speaker 3"}
- Labels used by male voices: {"Speaker 5", "Speaker 2", "Speaker 6", "Speaker 4"}
- Labels shared across genders (confusions): []
- Mic-is-me: 20/20 mic segments labelled You


- Loaded `ivrit-large-v3-turbo` in 2.2 s

## Per-clip transcription (ivrit large-v3-turbo, language=he, encoder context: full:full:1:1.0)

Clips: 10 · audio 85 s · compute 174 s · **RTF 2.06** · **WER 23.8%** (39 errors / 164 words)

| clip | voice | s | RTF | WER | reference | recognised |
|---|---|---|---|---|---|---|
| 1620 | male | 16.1 | 1.17 | 40% | כעת זמין באופן נרחב בכל רחבי הארכיפלג, המטבח הג'אווני מאופיין במגוון מנות מתובלות בפשטות, והטעמים הבולטים שהג'אוונסים אוהבים הם בוטנים, פלפל צ'ילי, סוכר (בעיקר סוכר קוקוס ג'אווני) ומגוון תבלינים ארומטיים. | כעת זמין באופן רחב בכל רחבי הארכי פלג, המטבח הג'וואני מאופיין במגוון מנות מטובלות בפשטות, והטעמים הבולטים שהג' וואנסנים אוהבים הם בוטנים, פלפל צ'ילי, סוכר ומגוון תבלינים אומותיים. |
| 1617 | male | 13.3 | 1.32 | 41% | הפארק הלאומי קרוגר (KNP) נמצא בחלק הצפון מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבבואה בצפון, וגבולו הדרומי הוא נהר קרוקודייל. | נמצא בחלק הצפון-מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבאווי בצפון, וגבולו הדרומי הוא נהר קאמבי, הפארק הלאומי קרוגר קרוקודייל. |
| 1592 | male | 7.6 | 2.17 | 0% | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים, בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. | כשהגעת למדינה זרה בהתחלה כנראה שאנשים היו סבלניים ומבינים בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. |
| 1651 | male | 5.2 | 3.45 | 0% | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. |
| 1552 | male | 6.5 | 2.58 | 0% | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי. | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי |
| 1634 | male | 8.1 | 2.17 | 10% | בבעלותה של יפן כמעט 7,000 איים (שהגדול מביניהם הוא הונשו), מה שהופך את יפן לאי ה-7 בגודלו בעולם! | בבעלותה של יפן כמעט 7,000 איים, שהגדול מביניהם הוא הונשו, מה שהופך את יפן לאי השבע בגודלו בעולם. |
| 1642 | male | 5.2 | 3.10 | 7% | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפיליפינים שולמו על-ידי העם הפיליפיני עצמו. | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפלפינים שולמו על ידי העם הפיליפיני עצמו. |
| 1597 | male | 7.6 | 2.34 | 42% | ניתן להמיר כסף בבנק היחידי באיים שממוקם בסטנלי מול חנות FIC West. | FIC ווסט ניתן להמיר כסף בבנק היחידי בעיים שממוקם בסטנלי מול חנות. |
| 1549 | male | 8.0 | 2.21 | 12% | תיאוריות אלה מצביעות על כך שלאנשים יש צרכים ו/או רצונות מסוימים שהם מפנימים ככל שהם מתבגרים. | תיאוריות אלה מצביעות על כך שלאנשים יש צרכים ואו רצונות מסוימים שהם מפנימים ככל שהם מתבגרים. |
| 1655 | male | 7.1 | 2.49 | 53% | \"למשל, נטען ש\"\"למידה\"\" ו\"\"חברות\"\" הם גורמי מוטיבציה חשובים לשימוש באינטרנט (ג'יימס ושות', 1995).\" | למשל ניתן שלמידה וחברות הם גורמי מוטיבציה חשובים לשימוש באינטרנט. |

## Per-clip transcription (ivrit large-v3-turbo, language=he, encoder context: dynamic:dynamic:1:1.0)

Clips: 10 · audio 85 s · compute 75 s · **RTF 0.89** · **WER 25.6%** (42 errors / 164 words)

| clip | voice | s | RTF | WER | reference | recognised |
|---|---|---|---|---|---|---|
| 1620 | male | 16.1 | 0.67 | 47% | כעת זמין באופן נרחב בכל רחבי הארכיפלג, המטבח הג'אווני מאופיין במגוון מנות מתובלות בפשטות, והטעמים הבולטים שהג'אוונסים אוהבים הם בוטנים, פלפל צ'ילי, סוכר (בעיקר סוכר קוקוס ג'אווני) ומגוון תבלינים ארומטיים. | כעת זמין באופן רחב בחול רחבי הארכי פלג, המטבח הג'וואני מאופיין במגוון מנות מטובלות בפשטות, והטעמים הבולטים שה ג'וואנסנים אוהבים הם בוטנים, פלפל צ'ילי, סוכר ומגוון טבלינים אומותיים. |
| 1617 | male | 13.3 | 0.60 | 41% | הפארק הלאומי קרוגר (KNP) נמצא בחלק הצפון מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבבואה בצפון, וגבולו הדרומי הוא נהר קרוקודייל. | נמצא בחלק הצפון-מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבאווי בצפון, וגבולו הדרומי הוא נהר קאנבי, הפארק הלאומי קרוגר קרוקודייל. |
| 1592 | male | 7.6 | 1.15 | 0% | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים, בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. |
| 1651 | male | 5.2 | 1.59 | 0% | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. |
| 1552 | male | 6.5 | 0.83 | 0% | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי. | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי. |
| 1634 | male | 8.1 | 0.63 | 10% | בבעלותה של יפן כמעט 7,000 איים (שהגדול מביניהם הוא הונשו), מה שהופך את יפן לאי ה-7 בגודלו בעולם! | בבעלותה של יפן כמעט 7,000 איים, שהגדול מביניהם הוא הונשו, מה שהופך את יפן לאי השבע בגודלו בעולם. |
| 1642 | male | 5.2 | 1.22 | 7% | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפיליפינים שולמו על-ידי העם הפיליפיני עצמו. | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפלפינים שולמו על ידי העם הפיליפיני עצמו. |
| 1597 | male | 7.6 | 0.98 | 42% | ניתן להמיר כסף בבנק היחידי באיים שממוקם בסטנלי מול חנות FIC West. | FIC ווסט ניתן להמיר כסף בבנק היחידי בעיים שממוקם בסטנלי מול חנות. |
| 1549 | male | 8.0 | 0.98 | 18% | תיאוריות אלה מצביעות על כך שלאנשים יש צרכים ו/או רצונות מסוימים שהם מפנימים ככל שהם מתבגרים. | תאוריות אלה מצביעות על כך שלאנשים יש צרכים ואו רצונות מסוימים שהם מפנימים ככל שהם מתבגרים. |
| 1655 | male | 7.1 | 1.07 | 53% | \"למשל, נטען ש\"\"למידה\"\" ו\"\"חברות\"\" הם גורמי מוטיבציה חשובים לשימוש באינטרנט (ג'יימס ושות', 1995).\" | למשל ניתן שלמידה וחברות הם גורמי מוטיבציה חשובים לשימוש באינטרנט. |

- Loaded `ivrit-zipformer-66m-he` in 1.2 s

## ivrit.ai streaming Zipformer (66M int8) per clip

Clips: 10 · audio 85 s · compute 6.8 s · **RTF 0.080** · **WER 28.0%** (46 errors / 164 words)

| clip | s | RTF | WER | reference | recognised |
|---|---|---|---|---|---|
| 1620 | 16.1 | 0.08 | 37% | כעת זמין באופן נרחב בכל רחבי הארכיפלג, המטבח הג'אווני מאופיין במגוון מנות מתובלות בפשטות, והטעמים הבולטים שהג'אוונסים אוהבים הם בוטנים, פלפל צ'ילי, סוכר (בעיקר סוכר קוקוס ג'אווני) ומגוון תבלינים ארומטיים. | כעת זמין באופן נרחב, בכל רחבי הארכי פלג המטבח הג'ואני מאופיין במגוון מנות מטובלות בפשטות, והטעמים הבולטים שהג'וונסנים אוהבים הם בוטנים בלבל צ'ילי סוכר ומגוון תבלינים האומותי. |
| 1617 | 13.3 | 0.10 | 41% | הפארק הלאומי קרוגר (KNP) נמצא בחלק הצפון מזרחי של דרום אפריקה, לאורך גבול מוזמביק במזרח, זימבבואה בצפון, וגבולו הדרומי הוא נהר קרוקודייל. | נמצא בחלק הצפון-מזרחי של דרום אפריקה לאורך גבול מוזמביק במזרח, זימבאווי בצפון, וגבולו הדרומי הוא נהר כאמביה, פארק הלאומי קרוגר קרוקודייל |
| 1592 | 7.6 | 0.12 | 0% | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים, בידיעה שמטיילים במדינות חדשות צריכים להתאקלם. | כשהגעת למדינה זרה בהתחלה, כנראה שאנשים היו סבלניים ומבינים בידיעה שמטיילים במדינות חדשות צריכים להתאקלם |
| 1651 | 5.2 | 0.09 | 0% | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. | אפשרות שנהיית יותר ויותר פופולרית לאלו שמתכננים שנת חופש היא לטייל וללמוד. |
| 1552 | 6.5 | 0.07 | 0% | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי. | תוצאות ניתוח הגרף יפורסמו באתר אינטרנט ציבורי. |
| 1634 | 8.1 | 0.06 | 25% | בבעלותה של יפן כמעט 7,000 איים (שהגדול מביניהם הוא הונשו), מה שהופך את יפן לאי ה-7 בגודלו בעולם! | בבעלותה של יפן כמעט 7,00 אימא, שהגדול מביניהם הוא הונשון, מה שהופך את יפן לאי השבע בגודלו בעולם |
| 1642 | 5.2 | 0.09 | 7% | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הפיליפינים שולמו על-ידי העם הפיליפיני עצמו. | הוצאות המלחמה של האימפריאליזם האמריקני על כיבוש הבפנים שולמו על ידי העם הפיליפיני עצמו. |
| 1597 | 7.6 | 0.06 | 42% | ניתן להמיר כסף בבנק היחידי באיים שממוקם בסטנלי מול חנות FIC West. | FAC West ניתן להמיר כסף בבנק היחידי באיים שממוקם בסטנדים מול חנות |
| 1549 | 8.0 | 0.07 | 29% | תיאוריות אלה מצביעות על כך שלאנשים יש צרכים ו/או רצונות מסוימים שהם מפנימים ככל שהם מתבגרים. | תיאוריות אלה מצביעות על כך שלאנשים יש צרכים ורצונות מסוימים שמפנינים ככל שהם מתבגרים |
| 1655 | 7.1 | 0.06 | 67% | \"למשל, נטען ש\"\"למידה\"\" ו\"\"חברות\"\" הם גורמי מוטיבציה חשובים לשימוש באינטרנט (ג'יימס ושות', 1995).\" | ניתן שלמידה וחברות הם גורמים מוטיבציה חשובים לשימוש באינטרנט |

## Run log excerpts

```text
[1m[33mwarning[0m: tamlil@0.4.1: ✅ macOS: Metal GPU acceleration ENABLED by default
whisper_init_with_params_no_state: backends   = 3
whisper_init_with_params_no_state: backends   = 3
whisper_model_load:    Metal total size =  1623.92 MB
whisper_backend_init: using BLAS backend
whisper_init_state: failed to load Core ML model from '/Users/runner/work/Tamlil/Tamlil/qa-models/ggml-tiny-encoder.mlmodelc'
whisper_backend_init_gpu: using Metal backend
ggml_metal_init: allocating
ggml_metal_init: picking default device: Apple Paravirtual device
ggml_metal_init: using embedded metal library
ggml_metal_init: GPU name:   Apple Paravirtual device
ggml_metal_init: GPU family: MTLGPUFamilyApple5  (1005)
ggml_metal_init: GPU family: MTLGPUFamilyCommon3 (3003)
ggml_metal_init: simdgroup reduction support   = false
ggml_metal_init: simdgroup matrix mul. support = false
ggml_metal_init: hasUnifiedMemory              = true
ggml_metal_init: recommendedMaxWorkingSetSize  =  5010.80 MB
ggml_metal_init: skipping kernel_soft_max_f16                      (not supported)
ggml_metal_init: skipping kernel_soft_max_f16_4                    (not supported)
ggml_metal_init: skipping kernel_soft_max_f32                      (not supported)
ggml_metal_init: skipping kernel_soft_max_f32_4                    (not supported)
ggml_metal_init: skipping kernel_rms_norm                          (not supported)
ggml_metal_init: skipping kernel_group_norm                        (not supported)
ggml_metal_init: skipping kernel_mul_mv_f32_f32                    (not supported)
ggml_metal_init: skipping kernel_mul_mv_f16_f16                    (not supported)
ggml_metal_init: skipping kernel_mul_mv_f16_f32                    (not supported)
ggml_metal_init: skipping kernel_mul_mv_f16_f32_1row               (not supported)
ggml_metal_init: skipping kernel_mul_mv_f16_f32_l4                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_q4_0_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q4_1_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q5_0_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q5_1_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q8_0_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q2_K_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q3_K_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q4_K_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q5_K_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_q6_K_f32                   (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq2_xxs_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq2_xs_f32                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq3_xxs_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq3_s_f32                  (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq2_s_f32                  (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq1_s_f32                  (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq1_m_f32                  (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq4_nl_f32                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_iq4_xs_f32                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_f32_f32                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_f16_f32                 (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q4_0_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q4_1_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q5_0_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q5_1_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q8_0_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q2_K_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q3_K_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q4_K_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q5_K_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_q6_K_f32                (not supported)
ggml_metal_init: skipping kernel_mul_mv_id_iq2_xxs_f32             (not supported)
```
Result: success
