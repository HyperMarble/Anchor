use anchor::graph::CodeGraph;
use anchor::parser::extract_file;
use std::path::PathBuf;

fn build(file: &str, src: &str) -> CodeGraph {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    g
}

#[test]
fn test_go_functions_and_structs() {
    let src = r#"
package main

import "fmt"

type Server struct {
    port int
    host string
}

func NewServer(host string, port int) *Server {
    return &Server{host: host, port: port}
}

func (s *Server) Start() {
    fmt.Println("starting")
}

func main() {
    srv := NewServer("localhost", 8080)
    srv.Start()
}
"#;
    let g = build("main.go", src);
    let results = g.search("Server", 5);
    assert!(!results.is_empty());
    let results = g.search("NewServer", 5);
    assert!(!results.is_empty());
    let results = g.search("main", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_go_interface_extraction() {
    let src = r#"
package storage

type Repository interface {
    Save(item interface{}) error
    FindByID(id string) (interface{}, error)
    Delete(id string) error
}

type InMemoryRepo struct {
    data map[string]interface{}
}

func (r *InMemoryRepo) Save(item interface{}) error {
    return nil
}
"#;
    let g = build("storage.go", src);
    let results = g.search("Repository", 5);
    assert!(!results.is_empty());
    let results = g.search("InMemoryRepo", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_java_class_and_methods() {
    let src = r#"
package com.example;

import java.util.List;
import java.util.ArrayList;

public class OrderService {
    private List<String> orders = new ArrayList<>();

    public void addOrder(String order) {
        orders.add(order);
    }

    public List<String> getOrders() {
        return orders;
    }

    public static OrderService create() {
        return new OrderService();
    }
}
"#;
    let g = build("OrderService.java", src);
    let results = g.search("OrderService", 5);
    assert!(!results.is_empty());
    let results = g.search("addOrder", 5);
    assert!(!results.is_empty());
    let results = g.search("getOrders", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_java_interface_and_enum() {
    let src = r#"
package com.example;

public interface Payable {
    double getAmount();
    void process();
}

public enum PaymentStatus {
    PENDING,
    COMPLETED,
    FAILED
}
"#;
    let g = build("Payment.java", src);
    let results = g.search("Payable", 5);
    assert!(!results.is_empty());
    // File was indexed
    assert!(g.stats().file_count > 0);
}

#[test]
fn test_csharp_class_and_methods() {
    let src = r#"
using System;
using System.Collections.Generic;

namespace Anchor.Services
{
    public class UserRepository
    {
        private List<string> _users = new List<string>();

        public void Add(string user)
        {
            _users.Add(user);
        }

        public string FindById(int id)
        {
            return _users[id];
        }
    }
}
"#;
    let g = build("UserRepository.cs", src);
    let results = g.search("UserRepository", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_swift_class_and_functions() {
    let src = r#"
import Foundation

struct Point {
    var x: Double
    var y: Double
}

class Shape {
    var origin: Point

    init(origin: Point) {
        self.origin = origin
    }

    func area() -> Double {
        return 0.0
    }
}

func distance(from a: Point, to b: Point) -> Double {
    return sqrt(pow(b.x - a.x, 2) + pow(b.y - a.y, 2))
}
"#;
    let g = build("Geometry.swift", src);
    // Swift file is parsed and indexed without errors
    assert!(g.stats().file_count > 0);
}

#[test]
fn test_tsx_react_components() {
    let src = r#"
import React, { useState } from 'react';

interface ButtonProps {
    label: string;
    onClick: () => void;
}

const Button: React.FC<ButtonProps> = ({ label, onClick }) => {
    return <button onClick={onClick}>{label}</button>;
};

function App() {
    const [count, setCount] = useState(0);
    return <Button label="click" onClick={() => setCount(count + 1)} />;
}

export default App;
"#;
    let g = build("App.tsx", src);
    let results = g.search("App", 5);
    assert!(!results.is_empty());
    let results = g.search("Button", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_cpp_class_and_functions() {
    let src = r#"
#include <string>
#include <vector>

class Logger {
public:
    Logger(const std::string& name) : name_(name) {}

    void log(const std::string& message) {
        logs_.push_back(message);
    }

    std::vector<std::string> getLogs() const {
        return logs_;
    }

private:
    std::string name_;
    std::vector<std::string> logs_;
};

void setupLogging() {
    Logger logger("main");
    logger.log("initialized");
}
"#;
    let g = build("logger.cpp", src);
    let results = g.search("Logger", 5);
    assert!(!results.is_empty());
    let results = g.search("setupLogging", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_ruby_class_and_methods() {
    let src = r#"
class BankAccount
  attr_reader :balance

  def initialize(owner)
    @owner = owner
    @balance = 0
  end

  def deposit(amount)
    @balance += amount
  end

  def withdraw(amount)
    @balance -= amount if amount <= @balance
  end
end

def open_account(name)
  BankAccount.new(name)
end
"#;
    let g = build("bank.rb", src);
    let results = g.search("BankAccount", 5);
    assert!(!results.is_empty());
    let results = g.search("open_account", 5);
    assert!(!results.is_empty());
}
