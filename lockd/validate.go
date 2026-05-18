// Input validation for all request fields.
package main

import (
	"errors"
	"regexp"
	"slices"
	"strings"
)

const (
	defaultTTL = 300
	maxTTL     = 3600
	minTTL     = 1
)

var (
	agentRe  = regexp.MustCompile(`^[A-Za-z0-9-]{1,64}$`)
	symbolRe = regexp.MustCompile(`^[A-Za-z0-9_:]{1,256}$`)
)

func validatePath(p string) error {
	if p == "" {
		return errors.New("path is empty")
	}
	if strings.HasPrefix(p, "/") {
		return errors.New("path must be relative")
	}
	if slices.Contains(strings.Split(p, "/"), "..") {
		return errors.New("path contains ..")
	}
	return nil
}

func validateAgent(a string) error {
	if !agentRe.MatchString(a) {
		return errors.New("agent id must be alphanumeric+hyphens, max 64 chars")
	}
	return nil
}

func validateSymbol(s string) error {
	if !symbolRe.MatchString(s) {
		return errors.New("symbol must be alphanumeric+underscores+colons, max 256 chars")
	}
	return nil
}

func normTTL(ttl int) int {
	if ttl <= 0 {
		return defaultTTL
	}
	if ttl > maxTTL {
		return maxTTL
	}
	if ttl < minTTL {
		return minTTL
	}
	return ttl
}
