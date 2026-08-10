#!/usr/bin/env bash

# Pure policy helpers shared by the release verifier and its contract tests.
# Keep these functions free of filesystem and codesign calls so fixtures can be
# injected without needing a signing identity.

paper_float_classify_signature() {
  local metadata="$1"

  if grep -Eq '^[[:space:]]*(Signature=adhoc|TeamIdentifier=not set)[[:space:]]*$' <<<"$metadata"; then
    printf '%s\n' "adhoc"
    return
  fi

  if grep -Eq '^[[:space:]]*Authority=Developer ID Application:' <<<"$metadata"; then
    printf '%s\n' "developer_id_application"
    return
  fi

  if grep -Eq '^[[:space:]]*Authority=Apple Development:' <<<"$metadata"; then
    printf '%s\n' "apple_development"
    return
  fi

  printf '%s\n' "unknown"
}

paper_float_extract_team_identifier() {
  local metadata="$1"

  sed -nE 's/^[[:space:]]*TeamIdentifier=([^[:space:]]+)[[:space:]]*$/\1/p' <<<"$metadata" | head -n 1
}

paper_float_extract_identifier() {
  local metadata="$1"

  sed -nE 's/^[[:space:]]*Identifier=([^[:space:]]+)[[:space:]]*$/\1/p' <<<"$metadata" | head -n 1
}

paper_float_extract_designated_requirement() {
  local requirement_output="$1"

  sed -nE 's/^[[:space:]]*designated[[:space:]]*=>[[:space:]]*(.+)$/\1/p' <<<"$requirement_output" | head -n 1
}

paper_float_is_developer_id_designated_requirement() {
  local requirement_output="$1"
  local expected_team_identifier="${2:-}"
  local expected_identifier="${3:-}"
  local requirement
  requirement="$(paper_float_extract_designated_requirement "$requirement_output")"

  if ! [[ -n "$requirement" ]] ||
    grep -Eiq '(^|[[:space:]()])or([[:space:]()]|$)|cdhash' <<<"$requirement" ||
    ! [[ "$requirement" == *"anchor apple generic"* ]] ||
    ! [[ "$requirement" == *"certificate 1[field.1.2.840.113635.100.6.2.6]"* ]] ||
    ! [[ "$requirement" == *"certificate leaf[field.1.2.840.113635.100.6.1.13]"* ]] ||
    ! [[ "$requirement" == *"certificate leaf[subject.OU]"* ]]; then
    return 1
  fi

  if ! { [[ -z "$expected_team_identifier" ]] ||
    [[ "$requirement" == *"certificate leaf[subject.OU] = \"$expected_team_identifier\""* ]] ||
    [[ "$requirement" == *"certificate leaf[subject.OU] = $expected_team_identifier"* ]]; }; then
    return 1
  fi

  [[ -z "$expected_identifier" ]] ||
    [[ "$requirement" == *"identifier \"$expected_identifier\""* ]]
}

paper_float_build_developer_id_requirement() {
  local identifier="$1"
  local team_identifier="$2"

  if ! [[ "$identifier" =~ ^[A-Za-z0-9.-]+$ ]] ||
    ! [[ "$team_identifier" =~ ^[A-Z0-9]{10}$ ]]; then
    return 1
  fi

  printf '%s\n' "identifier \"$identifier\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"$team_identifier\""
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  case "${1:-}" in
    classify-signature)
      paper_float_classify_signature "$(cat)"
      ;;
    extract-team-identifier)
      paper_float_extract_team_identifier "$(cat)"
      ;;
    extract-identifier)
      paper_float_extract_identifier "$(cat)"
      ;;
    extract-designated-requirement)
      paper_float_extract_designated_requirement "$(cat)"
      ;;
    validate-developer-id-requirement)
      if paper_float_is_developer_id_designated_requirement "$(cat)" "${2:-}" "${3:-}"; then
        printf '%s\n' "valid"
      else
        printf '%s\n' "invalid"
        exit 1
      fi
      ;;
    build-developer-id-requirement)
      paper_float_build_developer_id_requirement "${2:-}" "${3:-}"
      ;;
    *)
      echo "usage: $0 {classify-signature|extract-team-identifier|extract-identifier|extract-designated-requirement|validate-developer-id-requirement|build-developer-id-requirement}" >&2
      exit 64
      ;;
  esac
fi
