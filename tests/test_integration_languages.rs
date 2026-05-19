use anchor::parser::extract_file;
use std::path::PathBuf;

fn names(file: &str, src: &str) -> Vec<String> {
    extract_file(&PathBuf::from(file), src)
        .unwrap()
        .symbols
        .into_iter()
        .map(|s| s.name)
        .collect()
}

#[test]
fn test_go_functions_and_structs() {
    let src = r#"
package main
import "fmt"
type Server struct { port int; host string }
func NewServer(host string, port int) *Server { return &Server{} }
func (s *Server) Start() { fmt.Println("starting") }
func main() {}
"#;
    let n = names("main.go", src);
    assert!(n.iter().any(|s| s == "Server"));
    assert!(n.iter().any(|s| s == "NewServer"));
    assert!(n.iter().any(|s| s == "main"));
}

#[test]
fn test_go_interface_extraction() {
    let src = r#"
package storage
type Repository interface { Save(item interface{}) error }
type InMemoryRepo struct { data map[string]interface{} }
func (r *InMemoryRepo) Save(item interface{}) error { return nil }
"#;
    let n = names("storage.go", src);
    assert!(n.iter().any(|s| s == "Repository"));
    assert!(n.iter().any(|s| s == "InMemoryRepo"));
}

#[test]
fn test_java_class_and_methods() {
    let src = r#"
package com.example;
import java.util.List;
public class OrderService {
    public void addOrder(String order) {}
    public List<String> getOrders() { return null; }
    public static OrderService create() { return new OrderService(); }
}
"#;
    let n = names("OrderService.java", src);
    assert!(n.iter().any(|s| s == "OrderService"));
    assert!(n.iter().any(|s| s == "addOrder"));
    assert!(n.iter().any(|s| s == "getOrders"));
}

#[test]
fn test_java_interface_and_enum() {
    let src = r#"
package com.example;
public interface Payable { double getAmount(); void process(); }
public enum PaymentStatus { PENDING, COMPLETED, FAILED }
"#;
    let n = names("Payment.java", src);
    assert!(n.iter().any(|s| s == "Payable"));
}

#[test]
fn test_csharp_class_and_methods() {
    let src = r#"
using System;
namespace Anchor.Services {
    public class UserRepository {
        public void Add(string user) {}
        public string FindById(int id) { return ""; }
    }
}
"#;
    let n = names("UserRepository.cs", src);
    assert!(n.iter().any(|s| s == "UserRepository"));
}

#[test]
fn test_tsx_react_components() {
    let src = r#"
import React, { useState } from 'react';
interface ButtonProps { label: string; onClick: () => void; }
const Button: React.FC<ButtonProps> = ({ label, onClick }) => { return null; };
function App() { return null; }
export default App;
"#;
    let n = names("App.tsx", src);
    assert!(n.iter().any(|s| s == "App"));
    assert!(n.iter().any(|s| s == "Button"));
}

#[test]
fn test_cpp_class_and_functions() {
    let src = r#"
#include <string>
class Logger {
public:
    Logger(const std::string& name) {}
    void log(const std::string& message) {}
};
void setupLogging() {}
"#;
    let n = names("logger.cpp", src);
    assert!(n.iter().any(|s| s == "Logger"));
    assert!(n.iter().any(|s| s == "setupLogging"));
}

#[test]
fn test_ruby_class_and_methods() {
    let src = r#"
class BankAccount
  def initialize(owner) end
  def deposit(amount) end
  def withdraw(amount) end
end
def open_account(name) end
"#;
    let n = names("bank.rb", src);
    assert!(n.iter().any(|s| s == "BankAccount"));
    assert!(n.iter().any(|s| s == "open_account"));
}
