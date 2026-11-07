# Hash Hunter

Hash Hunter는 Git 커밋 해시에서 회문, 같은 문자의 연속, 반복 패턴을 찾는 CLI입니다. SHA-1과 SHA-256을 지원하며 희귀도와 시각적 특징을 바탕으로 순위를 매깁니다.

[![Version 0.2.0](https://img.shields.io/badge/version-0.2.0-2563eb)](Cargo.toml) [![Rust 2024](https://img.shields.io/badge/Rust-2024-24292f?logo=rust&logoColor=white)](Cargo.toml) [![MIT License](https://img.shields.io/badge/license-MIT-15803d)](LICENSE)

[특징](#특징) · [설치](#설치) · [사용법](#사용법) · [점수](#점수) · [개발](#개발) · [라이선스](#라이선스)

[日本語](README.md) · [English](README.en.md) · [한국어](README.ko.md)

---

## 특징

- Git 이력이나 해시 목록을 배치 단위로 읽고 CPU에서 병렬로 점수를 계산합니다.
- 회문, 같은 문자의 연속, 주기 2~8의 반복, 순환하지 않는 16진수 수열, `deadbeef` 같은 사전 단어, 접두부와 접미부의 일치를 감지합니다. 특정 문자의 빈도가 높은 해시와 숫자 또는 `a-f`로만 이루어진 해시도 찾습니다.
- 터미널에서 상위 결과와 채점 근거를 확인할 수 있습니다. 기준을 충족하는 모든 결과는 CSV로 저장할 수 있습니다.

## 설치

안정 버전의 Rust와 Cargo가 필요합니다. Git 이력을 조사할 때는 PATH에 있는 Git을 사용합니다.

```sh
git clone https://github.com/nlaocs/hash-hunter.git
cd hash-hunter
cargo install --path . --locked
```

## 사용법

### Git 이력 조사

```sh
hash-hunter --repo "path/to/repository" --top 100
hash-hunter --repo "path/to/repository" --sort rarity --min-bits 16 --csv findings.csv
```

`--repo`를 생략하면 현재 디렉터리의 저장소를 사용합니다. `git rev-list --all`을 실행해 로컬 참조와 HEAD에서 도달할 수 있는 커밋을 읽습니다.

진행 상황은 표준 오류 출력에 표시됩니다. `Reading Git history`는 읽은 해시 수와 경과 시간을, `Scoring`은 백분율, 처리 건수, 경과 시간, 예상 남은 시간(ETA)을 보여 줍니다. 이력을 읽는 동안에는 총건수를 모르므로 채점이 시작된 뒤 백분율을 표시합니다.

정확한 총건수를 얻기 위해 Git의 해시 목록을 임시 파일에 저장한 뒤 채점합니다. Git의 이력 열거는 한 번만 실행하며, 임시 파일은 닫을 때 삭제됩니다. 약 150만 커밋의 Linux 이력은 약 62 MB의 임시 저장 공간을 사용합니다.

기본 목록은 해시 하나를 한 줄에 표시하며 점수, 해시, 주요 패턴을 보여 줍니다. Beauty의 근거나 추가 특징을 별도 줄에 끼워 넣지 않습니다. 다음은 Linux 이력을 조사한 출력의 일부입니다.

```text
Score  Hash                                      Pattern
40.38  23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03  周期5:d9d99d9d99d9 + 回文:9d99d9d99d9
38.92  5f5837d2d650db25b9153b91535e67a96b265f58  周期5:b9153b9153 + 先頭=末尾:5f58
```

`--columns`로 표시할 항목과 순서를 지정합니다. `score`, `bits`, `beauty`, `expected`, `hash`, `pattern`을 쉼표로 구분해 선택할 수 있습니다. 표시 항목을 바꿔도 채점이나 순위는 바뀌지 않습니다.

```sh
hash-hunter --repo "path/to/repository" --columns score,hash
hash-hunter --repo "path/to/repository" --columns score,bits,beauty,expected,hash,pattern
hash-hunter --repo "path/to/repository" --details
```

`--details`는 해시마다 점수와 자세한 채점 근거를 여러 줄로 모아 표시합니다. 복합 패턴의 요소는 줄을 나누고 두 근거에 아직 표시되지 않은 특징도 보여 줍니다. `--columns`와 함께 지정할 수 없습니다.

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

CSV 행은 입력 순서대로 기록되며 UTF-8 BOM을 포함합니다. 출력 경로에 기존 파일이 있으면 조사와 쓰기가 성공한 뒤 교체합니다.

### 해시 목록 읽기

`--stdin`에 한 줄에 하나씩 소문자 16진수 해시를 전달합니다. 40자리와 64자리를 지원하며 LF와 CRLF를 모두 읽을 수 있습니다. PowerShell에서는 다음과 같이 실행합니다.

```powershell
Get-Content .\hashes.txt | hash-hunter --stdin --threads 8
```

`--stdin`은 해시를 읽으면서 채점하고 처리 건수와 경과 시간을 표시합니다. 총건수를 모르므로 완료할 때 100%를 표시합니다.

### 해시 하나의 채점 근거 보기

```sh
hash-hunter --explain d84a73bc621f325500005523d89b6e04f13a2cb8
```

출력의 일부입니다.

```text
Rarity       : 23.142 bits
Beauty       : 94 / 100 (heuristic)
Hybrid       : 34.422 (weight=0.12)
Fixed layout : 28.000 bits
Placements   : 29 (log2=4.858)
```

이 해시에는 `325500005523`이라는 회문이 있습니다. 회문의 제약은 24 bits이고 가운데의 `0000`이 독립적인 제약 4 bits를 더합니다. 40자리 해시에서 가능한 29개 위치를 반영하면 Rarity는 약 23.14 bits입니다.

### 옵션

| 옵션 | 용도 | 기본값 |
| --- | --- | --- |
| `--repo PATH` | Git 저장소 지정 | 현재 디렉터리 |
| `--stdin` | 표준 입력에서 해시 목록 읽기 | — |
| `--explain HASH` | 해시 하나의 채점 근거 표시 | — |
| `--top N` | 터미널에 표시할 상위 결과 수 | 50 |
| `--threads N` | 채점 스레드 수 | 사용 가능한 논리 CPU 수와 8 중 작은 값 |
| `--min-bits X` | 결과에 포함할 최소 Rarity | 12 bits |
| `--sort MODE` | `rarity`, `beauty`, `hybrid` 중 순위 기준 선택 | `hybrid` |
| `--beauty-weight X` | Hybrid에서 Beauty에 곱하는 계수 | 0.12 |
| `--focus-digits HEX` | 지정한 문자의 연속을 별도의 패턴군으로 채점 | 지정 없음 |
| `--columns FIELDS` | 목록의 표시 항목과 순서를 쉼표로 구분해 지정 | `score,hash,pattern` |
| `--details` | 점수, 채점 근거, 추가 특징을 여러 줄로 자세히 표시 | — |
| `--csv PATH` | 기준을 충족하는 모든 결과를 CSV로 저장 | — |
| `--help` / `--version` | 도움말 또는 버전 표시 | — |

### 큰 이력을 반복해서 조사하기

Git 커밋 그래프를 만들면 이력을 열거하는 시간을 줄일 수 있습니다. 이 명령은 저장소의 Git 메타데이터에 인덱스를 작성합니다. [Git 문서](https://git-scm.com/docs/git-commit-graph)

```sh
git -C "path/to/repository" commit-graph write --reachable
```

같은 이력에 여러 채점 조건을 적용하려면 `git rev-list --all`의 출력을 한 번 저장한 뒤 `--stdin`에 전달할 수 있습니다.

## 점수

- **Rarity**는 패턴군의 출현 확률 상한을 `q`라고 할 때 `-log2(q)` bits입니다. 각 16진수 자리는 독립적인 균등 난수라고 가정합니다. 떨어져 있는 두 패턴은 겹치지 않는 위치만 셉니다. 두 패턴이 같은 패턴군에 속하면 각 위치 쌍을 한 번만 셉니다. 겹치는 경우에는 상대 위치를 고정하고 동시에 성립하는 단어나 수열의 조합을 셉니다.
- **Beauty**는 패턴의 길이와 대칭성 같은 시각적 특징을 0~100점으로 평가합니다. 겹치는 같은 종류의 특징은 하나의 시각적 패턴으로 취급합니다. Rarity와 Beauty는 각각 점수가 가장 높은 후보를 선택합니다. 두 근거가 다르면 `--details`와 `--explain`에서 모두 표시합니다. CSV의 `primary`는 Rarity의 근거, `beauty_feature`는 Beauty의 근거입니다. `composite`는 Rarity의 근거가 복합 패턴인지 나타냅니다.
- **Hybrid**는 `Rarity + beauty-weight × Beauty`입니다. 동점이면 Rarity, Beauty, 해시의 사전순을 차례대로 내림차순으로 비교합니다.
- **Expected**는 표시된 패턴군의 예상 검출 건수 상한입니다. `2^(-Rarity)`에 길이가 같은 조사 대상 해시 수를 곱합니다. 소수로 표시하며 백만분의 일보다 작은 값은 `<0.000001`로 표시합니다.

양 끝의 동일 문자 연속과 내부의 동일 문자 연속은 별도의 패턴군입니다. 홀수 길이 회문의 가운데에 있는 세 문자의 연속은 해당 회문과 함께 채점합니다. `--focus-digits`는 Rarity에 반영합니다. 해시 하나당 지정 문자 보정 전의 Rarity가 높은 부분 패턴 후보를 최대 24개 유지하며 동점이면 감지 순서를 따릅니다. 단일 패턴, 두 패턴의 조합, 회문과 그 안에 있는 두 패턴을 합친 조합을 채점합니다. 세 패턴의 조합에서 내부 패턴 중 하나 이상은 같은 문자의 연속입니다.

## 개발

```sh
cargo build --release --locked
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

## 라이선스

Hash Hunter는 MIT 라이선스로 배포됩니다. [LICENSE](LICENSE)
