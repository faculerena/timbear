#!/bin/bash

BASE_URL="http://localhost:3000"

function show_usage() {
    echo "uso:"
    echo "  ./timbear.sh create <name>       crea timba (con lo que haya en fields.json)"
    echo "  ./timbear.sh vote <id> <choice>  vota en una timba"
    echo "  ./timbear.sh get <id>            status de la timba"
    echo ""
}

function create_poll() {
    local name="$1"

    if [ -z "$name" ]; then
        echo "error, pasame un nombre"
        exit 1
    fi

    if [ ! -f "fields.json" ]; then
        echo "error, no hay un fields.json"
        exit 1
    fi

    echo "Creating poll '$name'..."
    response=$(curl -s -X POST "$BASE_URL/new/$name" \
        -H "Content-Type: application/json" \
        -d @fields.json)

    echo "$response" | jq -r '
        if .id then
            "timba creada\n ID: \(.id)\n\nusa ese ID para votar"
        else
            "error timbeando :(\n\(.message)"
        end
    '
}

function vote_poll() {
    local id="$1"
    local choice="$2"

    if [ -z "$id" ] || [ -z "$choice" ]; then
        echo "error, pasame id y choice"
        exit 1
    fi

    echo "votando '$choice' en la timba $id..."
    response=$(curl -s -X POST "$BASE_URL/vote/$id/$choice")

    echo "$response" | jq -r '
        if .success then
            "OK: \(.message)"
        else
            "ERR: \(.message)"
        end
    '
}

function get_results() {
    local id="$1"

    if [ -z "$id" ]; then
        echo "error, pasame un id"
        exit 1
    fi

    echo "buscando resultados para timba $id..."
    echo ""

    response=$(curl -s "$BASE_URL/timba/$id")

    # Check if poll exists
    if echo "$response" | jq -e . >/dev/null 2>&1; then
        if echo "$response" | jq -e '.id' >/dev/null 2>&1; then
            echo "$response" | jq -r '
                "Timba: \(.name)",
                "ID: \(.id)",
                "Tipo: \(.type)",
                "Creada: \(.time_of_creation)",
                (if .deadline then "Deadline: \(.deadline)" else "" end),
                "",
                "Resultados:",
                "--------",
                (.votes | sort_by(-.count) | .[] | "  \(.choice): \(.count) vote(s)"),
                "",
                "Total: \(.total_votes)"
            '
        else
            echo "ERR: La timba no existe"
        fi
    else
        echo "ERR: ERR"
    fi
}

# Main command dispatcher
case "$1" in
    create)
        create_poll "$2"
        ;;
    vote)
        vote_poll "$2" "$3"
        ;;
    get)
        get_results "$2"
        ;;
    help|--help|-h|"")
        show_usage
        ;;
    *)
        echo "Q HACESSSS NO SE LO QUE ES '$1'"
        echo ""
        show_usage
        exit 1
        ;;
esac
