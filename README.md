# Hash Hunter

Git のコミットハッシュから、回文、同じ文字の連続、周期的な繰り返しなどを検出して順位を付ける CLI です。SHA-1 の40桁と SHA-256 の64桁に対応します。

## ビルド

安定版の Rust と Cargo でビルドします。Git 履歴を調べるときは、PATH 上の Git を使います。

```sh
cargo build --release --locked
cargo install --path . --locked
```

`cargo install` を使うと、`hash-hunter` コマンドを Cargo の bin ディレクトリから実行できます。

## 使い方

```powershell
hash-hunter --repo "D:\Projects\example" --top 100 --details
hash-hunter --repo "D:\Projects\example" --sort rarity --min-bits 16 --csv findings.csv
hash-hunter --focus-digits 0 --explain d84a73bc621f325500005523d89b6e04f13a2cb8
Get-Content .\hashes.txt | hash-hunter --stdin --threads 8
```

引数を省略すると、カレントディレクトリのリポジトリを調べます。Git モードでは `git rev-list --all` を使い、ローカルの参照と HEAD から到達できるコミットを読み取ります。標準入力では、小文字の16進ハッシュを1行に1件指定します。LF と CRLF の両方を受け付けます。

| オプション | 用途 |
|---|---|
| `--top N` | 表示する上位件数。既定値は50 |
| `--threads N` | 採点に使うスレッド数。既定値は利用可能な論理 CPU 数と8の小さい方 |
| `--min-bits X` | 検出結果に含める Rarity の下限。既定値は12 bits |
| `--sort rarity\|beauty\|hybrid` | 順位に使うスコア。既定値は `hybrid` |
| `--beauty-weight X` | Hybrid の Beauty に掛ける係数。既定値は0.12 |
| `--focus-digits HEX` | 指定した文字の連続を別のパターン族として評価 |
| `--csv PATH` | しきい値を満たした全件を CSV に保存 |
| `--details` | 上位結果に追加の候補も表示 |
| `--help` | 全オプションと引数の制約を表示 |

順位は、選んだスコア、Rarity、Beauty、ハッシュの辞書順を順に降順で比較します。端末と `--explain` では Rarity の根拠を表示し、Beauty の根拠が異なる場合は別に表示します。CSV の `primary` と `beauty_feature` はそれぞれの根拠で、`composite` は Rarity の根拠が複合かを表します。

CSV は入力順で、Excel でも読み込めるよう UTF-8 BOM を付けます。出力先に既存ファイルがある場合は、走査と書き込みが成功した時点で置き換えます。

## スコア

検出するのは、同一文字の連続、周期2〜8の繰り返し、回文、折り返しのない16進連番、`deadbeef` などの固定ワード、先頭と末尾の一致、特定文字の高頻度、数字のみ・英字のみのハッシュです。

**Rarity** は、16進各桁を独立な一様乱数と仮定したときの、パターン族ごとの出現しにくさを bits で表します。局所パターンでは、固定位置で条件を満たす確率を数え、成立する配置の数を掛けて出現確率の上限を求めます。

- 離れたペアでは、互いに重ならない配置だけを数えます。同じ族同士の組は一度ずつ数えます。
- 重なる複合では、各特徴の相対位置を固定し、全体を置ける位置を数えます。固定位置の確率には、共通の桁で両立する辞書語や連番の組を含めます。
- ゾロ目は、端にあるものと内側にあるものを別の族として扱います。奇数桁の回文の中央にある3連のゾロ目は、その回文との複合に使います。

例えば `325500005523` は、回文だけで24 bits、中央の `0000` も合わせると重複を除いて28 bits です。40桁のハッシュでは29か所に置けるため、位置を補正した Rarity は約23.14 bits になります。

採点では、1ハッシュにつき最大24個の局所パターンを残し、単独・ペア・回文に含まれる三重複合を評価します。上限を超えた場合は、指定文字の補正を加える前の Rarity が高い候補を優先し、同点の候補は検出順に並べます。

`Expected` は、同じ桁数の調査件数に `2^(-Rarity)` を掛けた、表示中のパターン族の期待件数の上限です。小数で表示し、100万分の1件より小さい値は `<0.000001` と表示します。

**Beauty** はパターンの長さや対称性などから求める0〜100の点数です。重なる同種の特徴は一つの見た目として扱い、Beauty が最大の候補を独立に選びます。`--focus-digits` は、指定文字のゾロ目が出る確率を Rarity に反映する設定です。**Hybrid** は Rarity に `beauty-weight × Beauty` を加えた値です。

## 検証

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

ライセンスは MIT です。[LICENSE](LICENSE)
