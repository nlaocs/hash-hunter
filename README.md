# Hash Hunter

Git のコミットハッシュを走査し、回文、ゾロ目、繰り返しなどを見つける CLI です。SHA-1 と SHA-256 に対応し、珍しさと見た目の特徴をもとに順位を付けます。

[![Version 0.2.0](https://img.shields.io/badge/version-0.2.0-2563eb)](Cargo.toml) [![Rust 2024](https://img.shields.io/badge/Rust-2024-24292f?logo=rust&logoColor=white)](Cargo.toml) [![MIT License](https://img.shields.io/badge/license-MIT-15803d)](LICENSE)

[特徴](#特徴) · [インストール](#インストール) · [使い方](#使い方) · [スコア](#スコア) · [開発](#開発) · [ライセンス](#ライセンス)

[日本語](README.md) · [English](README.en.md) · [한국어](README.ko.md)

---

## 特徴

- Git 履歴やハッシュ一覧をバッチ単位で読み込み、CPU で並列に採点します。
- 回文、ゾロ目、周期2〜8の繰り返し、折り返しのない16進連番、`deadbeef` などの辞書語、先頭と末尾の一致を検出します。文字の偏りや、数字のみ・`a-f` のみで構成されたハッシュも対象です。
- 端末では上位の結果と採点理由を確認できます。しきい値を満たした全件は CSV に保存できます。

## インストール

安定版の Rust と Cargo が必要です。Git 履歴を調べる場合は、PATH 上の Git を使います。

```sh
git clone https://github.com/nlaocs/hash-hunter.git
cd hash-hunter
cargo install --path . --locked
```

## 使い方

### Git 履歴を調べる

```sh
hash-hunter --repo "path/to/repository" --top 100
hash-hunter --repo "path/to/repository" --sort rarity --min-bits 16 --csv findings.csv
```

`--repo` を省略すると、カレントディレクトリのリポジトリを使います。`git rev-list --all` で、ローカルにある参照と HEAD から到達できるコミットを読み取ります。

処理中は標準エラー出力に進捗を表示します。`Reading Git history` は読み取った件数と経過時間、`Scoring` は採点のパーセント、処理件数、経過時間、推定残り時間（ETA）です。データの受信待ち中も経過時間を更新します。履歴の読み取り中は総件数がまだ分からないため、パーセントは採点の開始後に表示します。

正確な総件数を得るため、Git が列挙したハッシュを一時ファイルに保存してから採点します。Git の列挙は1回だけで、一時ファイルは閉じると削除されます。約150万コミットの Linux 履歴では約62 MBの一時領域を使います。

通常は1件1行で、点数、ハッシュ、主なパターンを表示します。Beauty の補足や追加の特徴は別行に挟みません。Linux の履歴を調べた出力の抜粋です。

```text
Score  Hash                                      Pattern
40.38  23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03  周期5:d9d99d9d99d9 + 回文:9d99d9d99d9
38.92  5f5837d2d650db25b9153b91535e67a96b265f58  周期5:b9153b9153 + 先頭=末尾:5f58
```

`--columns` で表示項目と順序を指定できます。`score`、`bits`、`beauty`、`expected`、`hash`、`pattern` をカンマ区切りで選びます。表示項目を変えても採点や順位付けは変わりません。

```sh
hash-hunter --repo "path/to/repository" --columns score,hash
hash-hunter --repo "path/to/repository" --columns score,bits,beauty,expected,hash,pattern
hash-hunter --repo "path/to/repository" --details
```

`--details` は1件ごとに点数と詳しい採点理由をまとめる複数行の表示です。複合パターンは要素ごとに改行し、採点理由に使われていない特徴も表示します。`--columns` と同時には指定できません。

```text
#1  23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03
    Score 40.38   Rarity 31.14 bits   Beauty 77/100
    Expected 0.000635 hits
    Rarity (入れ子の複合)
      - 周期5:d9d99d9d99d9
      - 回文:9d99d9d99d9
    Other
      - 回文:d9d99d9d
```

CSV は入力順に並び、UTF-8 BOM が付きます。出力先に既存ファイルがある場合は、走査と書き込みが成功した時点で置き換えます。

### ハッシュ一覧を読む

`--stdin` には、小文字の40桁または64桁の16進ハッシュを1行に1件渡します。LF と CRLF に対応します。PowerShell では次のように実行できます。

```powershell
Get-Content .\hashes.txt | hash-hunter --stdin --threads 8
```

`--stdin` は読みながら採点し、処理件数と経過時間を表示します。総件数が不明なため、完了時に100%を表示します。

### 1件の採点理由を見る

```sh
hash-hunter --explain d84a73bc621f325500005523d89b6e04f13a2cb8
```

出力の抜粋です。

```text
Rarity       : 23.142 bits
Beauty       : 94 / 100 (heuristic)
Hybrid       : 34.422 (weight=0.12)
Fixed layout : 28.000 bits
Placements   : 29 (log2=4.858)
```

このハッシュには `325500005523` という回文があります。回文の24 bitsに、中央の `0000` で新たに必要になる4 bitsを加えます。40桁のハッシュ内で成立する29か所を補正すると、Rarity は約23.14 bitsです。

### オプション

| オプション | 用途 | 既定値 |
| --- | --- | --- |
| `--repo PATH` | Git リポジトリを指定 | カレントディレクトリ |
| `--stdin` | 標準入力からハッシュ一覧を読む | — |
| `--explain HASH` | 1件の採点理由を表示 | — |
| `--top N` | 端末に表示する上位件数 | 50 |
| `--threads N` | 採点スレッド数 | 利用可能な論理 CPU 数と8の小さい方 |
| `--min-bits X` | 検出結果に含める Rarity の下限 | 12 bits |
| `--sort MODE` | `rarity`・`beauty`・`hybrid` から順位付けを選択 | `hybrid` |
| `--beauty-weight X` | Hybrid の Beauty に掛ける係数 | 0.12 |
| `--focus-digits HEX` | 指定した文字のゾロ目を別のパターン族として採点 | 指定なし |
| `--columns FIELDS` | 一覧の表示項目と順序をカンマ区切りで指定 | `score,hash,pattern` |
| `--details` | 点数、採点理由、追加の特徴を複数行で詳しく表示 | — |
| `--csv PATH` | しきい値を満たした全件を CSV に保存 | — |
| `--help` / `--version` | ヘルプまたはバージョンを表示 | — |

### 大きな履歴を繰り返し調べる

Git のコミットグラフを作成すると、履歴の列挙を短縮できます。このコマンドはリポジトリの Git メタデータに索引を作成します。 [Git のドキュメント](https://git-scm.com/docs/git-commit-graph)

```sh
git -C "path/to/repository" commit-graph write --reachable
```

同じ履歴で採点条件を変えて試す場合は、`git rev-list --all` の出力を一度保存し、`--stdin` に渡せます。

## スコア

- **Rarity** は、パターン族の出現確率の上限を `q` として `-log2(q)` bits で表します。計算では、16進各桁を独立な一様乱数と仮定します。離れたペアでは互いに重ならない配置を数え、同じ族同士の組は一度ずつ数えます。重なる複合では相対位置を固定し、その位置関係で両立する辞書語や連番の組を数えます。
- **Beauty** は、長さや対称性などから求める0〜100の点数です。重なる同種の特徴は一つの見た目として扱います。Rarity と Beauty はそれぞれ最大になる候補を選びます。`--details` と `--explain` は採点理由が異なる場合に両方を表示し、CSV では `primary` が Rarity の理由、`beauty_feature` が Beauty の理由です。`composite` は Rarity の理由が複合かを示します。
- **Hybrid** は `Rarity + beauty-weight × Beauty` です。同点の順位は Rarity、Beauty、ハッシュの辞書順を順に降順で比較します。
- **Expected** は、表示したパターン族の期待件数の上限です。同じ桁数の走査件数に `2^(-Rarity)` を掛け、小数で表示します。100万分の1件より小さい値は `<0.000001` と表示します。

端にあるゾロ目と内側にあるゾロ目は別の族です。奇数桁の回文の中央にある3連は、その回文との複合に使います。`--focus-digits` は Rarity に反映します。局所候補は1件につき最大24個で、指定文字の補正前の Rarity を優先して残し、同点は検出順です。単独、ペア、回文とその内側の2候補（少なくとも一方はゾロ目）を合わせた三重複合を採点します。

## 開発

```sh
cargo build --release --locked
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

## ライセンス

MIT ライセンスを適用しています。 [LICENSE](LICENSE)
